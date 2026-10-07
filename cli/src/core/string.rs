pub(crate) fn split_preserve_spaces(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();

    for c in input.chars() {
        if c == ' ' || c == '\n' || c == '\r' || c == '\t' {
            if !current.is_empty() {
                result.push(current.clone());
                current.clear();
            }
            result.push(c.to_string());
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Levenshtein edit distance, compared case-insensitively.
///
/// Only two rows of the matrix are kept, so this stays cheap enough to run
/// against every declared environment variable name in a workspace.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().flat_map(|c| c.to_lowercase()).collect();
    let b: Vec<char> = b.chars().flat_map(|c| c.to_lowercase()).collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current: Vec<usize> = vec![0; b.len() + 1];

    for (i, a_char) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + if a_char == b_char { 0 } else { 1 };
            let deletion = previous[j + 1] + 1;
            let insertion = current[j] + 1;
            current[j + 1] = substitution.min(deletion).min(insertion);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}

/// Minimum similarity, on a 0..1 scale, for a candidate to be worth
/// suggesting. 0.6 keeps one-word-off typos like `TWILIO_ACCOUNT_TOKEN` vs
/// `TWILIO_AUTH_TOKEN` (0.75) while dropping unrelated names.
const SUGGESTION_SIMILARITY_THRESHOLD: f64 = 0.6;

/// Rank `candidates` by how close they are to `input` and return at most
/// `limit` of them, nearest first.
///
/// Candidates that are not plausibly a typo of `input` are dropped entirely,
/// so an empty result means "no suggestion worth making" rather than "no
/// candidates".
pub(crate) fn closest_matches(input: &str, candidates: &[String], limit: usize) -> Vec<String> {
    let mut scored: Vec<(usize, &String)> = candidates
        .iter()
        .filter(|candidate| candidate.as_str() != input)
        .filter_map(|candidate| {
            let distance = levenshtein(input, candidate);
            let longest = input.chars().count().max(candidate.chars().count());
            if longest == 0 {
                return None;
            }
            let similarity = 1.0 - (distance as f64 / longest as f64);
            // Very short names never clear a ratio test, so allow a one or two
            // character slip on them regardless.
            if similarity >= SUGGESTION_SIMILARITY_THRESHOLD || distance <= 2 {
                Some((distance, candidate))
            } else {
                None
            }
        })
        .collect();

    // Ties are broken by name so the output is stable between runs.
    scored.sort_by(|(a_distance, a_name), (b_distance, b_name)| {
        a_distance.cmp(b_distance).then_with(|| a_name.cmp(b_name))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, name)| name.clone())
        .collect()
}

pub(crate) fn short_circuit_replacement(
    content: &str,
    replacements: &Vec<(String, String)>,
) -> String {
    split_preserve_spaces(content)
        .iter()
        .map(|word| {
            for (existing, new) in replacements {
                if word.contains(existing) {
                    return word.replace(existing, new);
                }
            }
            word.to_string()
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_levenshtein_identical_is_zero() {
        assert_eq!(levenshtein("DB_HOST", "DB_HOST"), 0);
    }

    #[test]
    fn test_levenshtein_is_case_insensitive() {
        assert_eq!(levenshtein("DB_HOST", "db_host"), 0);
    }

    #[test]
    fn test_levenshtein_empty_operand() {
        assert_eq!(levenshtein("", "ABC"), 3);
        assert_eq!(levenshtein("ABC", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_single_character_slip() {
        assert_eq!(levenshtein("DB_HOST", "DB_HOSTT"), 1);
        assert_eq!(levenshtein("DB_HOST", "DB_HST"), 1);
        assert_eq!(levenshtein("DB_HOST", "DB_HOSR"), 1);
    }

    /// The two real typos from the incident this exists for.
    #[test]
    fn test_closest_matches_surfaces_the_real_twilio_names() {
        let declared = names(&[
            "TWILIO_ACCOUNT_SID",
            "TWILIO_AUTH_TOKEN",
            "TWILIO_FROM_NUMBER",
            "STRIPE_API_KEY",
            "DB_HOST",
        ]);

        let suggestions = closest_matches("TWILIO_ACCOUNT_TOKEN", &declared, 3);
        assert!(suggestions.contains(&"TWILIO_AUTH_TOKEN".to_string()));
        assert!(suggestions.contains(&"TWILIO_ACCOUNT_SID".to_string()));

        let suggestions = closest_matches("TWILIO_AUTH_SID", &declared, 3);
        assert!(suggestions.contains(&"TWILIO_ACCOUNT_SID".to_string()));
        assert!(suggestions.contains(&"TWILIO_AUTH_TOKEN".to_string()));
    }

    #[test]
    fn test_closest_matches_drops_unrelated_names() {
        let declared = names(&["DB_HOST", "DB_PORT", "REDIS_URL"]);
        assert!(closest_matches("STRIPE_WEBHOOK_SECRET", &declared, 3).is_empty());
    }

    #[test]
    fn test_closest_matches_excludes_exact_input() {
        let declared = names(&["DB_HOST", "DB_PORT"]);
        let suggestions = closest_matches("DB_HOST", &declared, 3);
        assert!(!suggestions.contains(&"DB_HOST".to_string()));
    }

    #[test]
    fn test_closest_matches_orders_by_distance_then_name() {
        let declared = names(&["DB_HOSTS", "DB_HOST_NAME", "DB_HOSA"]);
        let suggestions = closest_matches("DB_HOST", &declared, 3);
        assert_eq!(suggestions[0], "DB_HOSA");
        assert_eq!(suggestions[1], "DB_HOSTS");
    }

    #[test]
    fn test_closest_matches_respects_limit() {
        let declared = names(&["DB_HOSTS", "DB_HOSA", "DB_HOSB", "DB_HOSC"]);
        assert_eq!(closest_matches("DB_HOST", &declared, 2).len(), 2);
    }

    #[test]
    fn test_closest_matches_empty_candidates() {
        assert!(closest_matches("DB_HOST", &[], 3).is_empty());
    }
}
