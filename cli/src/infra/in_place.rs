//! In-place edits for `forklaunch infra add|remove <service> object-store|cache`.
//!
//! These commands used to delegate to `change service --infrastructure`, which
//! reparses and reprints `registrations.ts` (dropping comments and the app's
//! formatting), reserializes both package.json files (reordering keys) and
//! rewrites the manifest from its struct (adding a header and stray keys). On a
//! real app that is a diff of hundreds of lines for a one-resource change.
//!
//! Everything here edits text instead: it inserts or removes only the entries,
//! import names, dependency lines and manifest keys the resource needs, in the
//! file's own style, and leaves every other byte alone. `remove` undoes what
//! `add` wrote, byte for byte.

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::constants::Infrastructure;

// ------------------------------------------------------------------ scanning

/// What each byte of a JS/TS/JSON source is: code, inside a string, or inside
/// a comment. Brackets and commas only count in code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Code,
    Str,
    Comment,
}

fn classify(text: &str) -> Vec<Class> {
    let b = text.as_bytes();
    let mut out = vec![Class::Code; b.len()];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = text[i..].find('\n').map(|n| i + n).unwrap_or(b.len());
                out[i..end].fill(Class::Comment);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..]
                    .find("*/")
                    .map(|n| i + 2 + n + 2)
                    .unwrap_or(b.len());
                out[i..end].fill(Class::Comment);
                i = end;
            }
            q @ (b'\'' | b'"' | b'`') => {
                let start = i;
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' {
                        i += 1;
                    } else if q != b'`' && b[i] == b'\n' {
                        break;
                    }
                    i += 1;
                }
                let end = (i + 1).min(b.len());
                out[start..end].fill(Class::Str);
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

fn matching_close(text: &str, class: &[Class], open: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0i32;
    for i in open..b.len() {
        if class[i] != Class::Code {
            continue;
        }
        match b[i] {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// One element of a comma-separated list: `start..end` is its text without
/// surrounding whitespace or comments; `comma` is the separator after it.
#[derive(Debug, Clone)]
struct Item {
    start: usize,
    end: usize,
    comma: Option<usize>,
}

fn push_item(
    text: &str,
    class: &[Class],
    from: usize,
    to: usize,
    comma: Option<usize>,
    items: &mut Vec<Item>,
) {
    let b = text.as_bytes();
    let significant = |i: usize| class[i] != Class::Comment && !b[i].is_ascii_whitespace();
    if let Some(start) = (from..to).find(|&i| significant(i)) {
        let end = (from..to).rev().find(|&i| significant(i)).unwrap() + 1;
        items.push(Item { start, end, comma });
    }
}

/// The top-level elements between the bracket at `open` and its match `close`.
fn list_items(text: &str, class: &[Class], open: usize, close: usize) -> Vec<Item> {
    let b = text.as_bytes();
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut from = open + 1;
    for i in open + 1..close {
        if class[i] != Class::Code {
            continue;
        }
        match b[i] {
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => depth -= 1,
            b',' if depth == 0 => {
                push_item(text, class, from, i, Some(i), &mut items);
                from = i + 1;
            }
            _ => {}
        }
    }
    push_item(text, class, from, close, None, &mut items);
    items
}

/// An object entry's key (`KEY: …`, `"key": …`) or a named import's name.
fn item_key(text: &str, item: &Item) -> String {
    let s = &text[item.start..item.end];
    if let Some(q) = s.chars().next().filter(|c| *c == '"' || *c == '\'') {
        return s[1..].split(q).next().unwrap_or_default().to_string();
    }
    let s = s.strip_prefix("type ").map(str::trim_start).unwrap_or(s);
    s.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect()
}

fn line_start(text: &str, i: usize) -> usize {
    text[..i].rfind('\n').map(|n| n + 1).unwrap_or(0)
}

fn indent_of(text: &str, i: usize) -> &str {
    let start = line_start(text, i);
    let line = &text[start..i];
    &line[..line.len() - line.trim_start().len()]
}

/// What goes between elements: a newline and the elements' indent for a
/// multi-line list, a space for a one-line list.
fn separator(text: &str, open: usize, items: &[Item]) -> String {
    let first = &items[0];
    if text[open..first.start].contains('\n') {
        format!("\n{}", indent_of(text, first.start))
    } else {
        " ".to_string()
    }
}

fn insert_before(text: &str, item: &Item, new: &str, sep: &str) -> String {
    format!("{}{new},{sep}{}", &text[..item.start], &text[item.start..])
}

fn append_item(text: &str, items: &[Item], new: &str, sep: &str) -> String {
    let last = items.last().expect("append to a non-empty list");
    match last.comma {
        Some(c) => format!("{}{sep}{new},{}", &text[..c + 1], &text[c + 1..]),
        None => format!("{},{sep}{new}{}", &text[..last.end], &text[last.end..]),
    }
}

/// Remove `items[idx]` and one separator, exactly undoing `insert_before` /
/// `append_item`.
fn remove_item(text: &str, items: &[Item], idx: usize) -> String {
    let item = &items[idx];
    let ls = line_start(text, item.start);
    let own_line = text[ls..item.start].trim().is_empty();
    let rest_blank = |from: usize| -> Option<usize> {
        let nl = text[from..].find('\n').map(|n| from + n)?;
        text[from..nl].trim().is_empty().then_some(nl)
    };
    if let Some(c) = item.comma {
        let from = if own_line { ls } else { item.start };
        let to = match rest_blank(c + 1) {
            Some(nl) if own_line => nl + 1,
            _ => {
                let skipped = text[c + 1..]
                    .bytes()
                    .take_while(|b| *b == b' ' || *b == b'\t')
                    .count();
                c + 1 + skipped
            }
        };
        return format!("{}{}", &text[..from], &text[to..]);
    }
    if idx > 0 {
        let prev_comma = items[idx - 1]
            .comma
            .expect("an element before the last has a comma");
        return format!("{}{}", &text[..prev_comma], &text[item.end..]);
    }
    match rest_blank(item.end) {
        Some(nl) if own_line => format!("{}{}", &text[..ls], &text[nl + 1..]),
        _ => format!("{}{}", &text[..item.start], &text[item.end..]),
    }
}

/// Where `new` goes in a sorted list of keys (ASCII, else case-insensitive):
/// `Some(Some(i))` before element i, `Some(None)` at the end, `None` when the
/// list is not sorted.
fn sorted_position(keys: &[String], new: &str) -> Option<Option<usize>> {
    if keys.windows(2).all(|w| w[0] <= w[1]) {
        return Some(keys.iter().position(|k| k.as_str() > new));
    }
    let lower: Vec<String> = keys.iter().map(|k| k.to_lowercase()).collect();
    if lower.windows(2).all(|w| w[0] <= w[1]) {
        let new = new.to_lowercase();
        return Some(lower.iter().position(|k| *k > new));
    }
    None
}

// ------------------------------------------------------------------ style

#[derive(Debug, Clone)]
struct Style {
    indent_unit: String,
    quote: char,
    trailing_commas: bool,
}

fn detect_style(text: &str) -> Style {
    let indent_unit = text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('*'))
        .map(|l| &l[..l.len() - l.trim_start().len()])
        .find(|i| !i.is_empty())
        .unwrap_or("  ")
        .to_string();
    let single = text.matches("from '").count() + text.matches("('").count();
    let double = text.matches("from \"").count() + text.matches("(\"").count();
    let quote = if double > single { '"' } else { '\'' };
    let trailing_commas = Regex::new(r",\s*\n\s*[}\])]").unwrap().is_match(text);
    Style {
        indent_unit,
        quote,
        trailing_commas,
    }
}

// ------------------------------------------------------ template snippets

/// The service template a fresh `forklaunch init` renders. The snippets `infra
/// add` inserts are cut from it, so both write the same registration.
const SERVICE_REGISTRATIONS_TEMPLATE: &str =
    include_str!("../templates/project/service/registrations.ts");

/// The bodies of `{{#tag}}…{{/tag}}` sections in the template.
fn template_sections(tag: &str) -> Vec<&'static str> {
    let open = format!("{{{{#{tag}}}}}");
    let close = format!("{{{{/{tag}}}}}");
    let mut out = Vec::new();
    let mut rest = SERVICE_REGISTRATIONS_TEMPLATE;
    while let Some(start) = rest.find(&open) {
        let body = &rest[start + open.len()..];
        let Some(end) = body.find(&close) else { break };
        out.push(&body[..end]);
        rest = &body[end + close.len()..];
    }
    out
}

/// The object entries of the template section that defines `key`, as
/// (key, text) with the template's own two-space indentation.
fn template_entries(tag: &str, key: &str) -> Result<Vec<(String, String)>> {
    let section = template_sections(tag)
        .into_iter()
        .find(|s| s.contains(&format!("{key}:")))
        .with_context(|| format!("service template has no {key} section"))?;
    let wrapped = format!("{{\n  {}\n}}", section.trim_start());
    let class = classify(&wrapped);
    let close = wrapped.len() - 1;
    Ok(list_items(&wrapped, &class, 0, close)
        .iter()
        .map(|item| {
            (
                item_key(&wrapped, item),
                wrapped[item.start..item.end].to_string(),
            )
        })
        .collect())
}

/// Fit a template entry to the file: its collector and encryption-key
/// names, quote style, trailing commas and indentation.
fn adapt_entry(entry: &str, file: &str, style: &Style, base_indent: &str) -> String {
    let mut text = entry.to_string();
    if !file.contains("LEGACY_ENCRYPTION_KEYS:") {
        let re = Regex::new(
            r"new FieldEncryptor\(ENCRYPTION_KEY,\s*\{\s*previousKeys:\s*parseEncryptionKeyList\(LEGACY_ENCRYPTION_KEYS\)\s*\}\)",
        )
        .unwrap();
        text = re
            .replace_all(&text, "new FieldEncryptor(ENCRYPTION_KEY)")
            .into_owned();
        text = Regex::new(r",\s*LEGACY_ENCRYPTION_KEYS\b")
            .unwrap()
            .replace_all(&text, "")
            .into_owned();
    }
    if !file.contains("OtelCollector:") && file.contains("OpenTelemetryCollector:") {
        text = Regex::new(r"\bOtelCollector\b")
            .unwrap()
            .replace_all(&text, "OpenTelemetryCollector")
            .into_owned();
    }
    if !style.trailing_commas {
        text = Regex::new(r",(\s*[}\])])")
            .unwrap()
            .replace_all(&text, "$1")
            .into_owned();
    }
    let other = if style.quote == '\'' { '"' } else { '\'' };
    let lines: Vec<String> = text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line = if line.trim_start().starts_with("//") {
                line.to_string()
            } else {
                line.replace(other, &style.quote.to_string())
            };
            if i == 0 {
                return line;
            }
            let spaces = line.len() - line.trim_start_matches(' ').len();
            let relative = spaces.saturating_sub(2);
            format!(
                "{base_indent}{}{}{}",
                style.indent_unit.repeat(relative / 2),
                " ".repeat(relative % 2),
                &line[spaces..]
            )
        })
        .collect();
    lines.join("\n")
}

// ---------------------------------------------------------- the resources

struct Spec {
    /// Template section tag.
    tag: &'static str,
    env_first_key: &'static str,
    runtime_first_key: &'static str,
    /// The import only this resource uses; removed with it.
    own_import: (&'static str, &'static [&'static str]),
}

fn spec(infrastructure: &Infrastructure) -> Spec {
    match infrastructure {
        Infrastructure::S3 => Spec {
            tag: "is_s3_enabled",
            env_first_key: "S3_REGION",
            runtime_first_key: "ObjectStore",
            own_import: (
                "@forklaunch/infrastructure-s3",
                &["S3ObjectStore", "s3ClientConfig"],
            ),
        },
        Infrastructure::Redis => Spec {
            tag: "is_request_cache_needed",
            env_first_key: "REDIS_URL",
            runtime_first_key: "TtlCache",
            own_import: ("@forklaunch/infrastructure-redis", &["RedisTtlCache"]),
        },
    }
}

/// Names the registration reads that other code may import too: added when
/// missing, dropped on remove only when nothing else uses them.
fn shared_imports(file: &str, app_name: &str) -> Vec<(String, Vec<&'static str>)> {
    let mut persistence = vec!["FieldEncryptor"];
    if file.contains("LEGACY_ENCRYPTION_KEYS:") {
        persistence.push("parseEncryptionKeyList");
    }
    vec![
        ("@forklaunch/core/persistence".to_string(), persistence),
        (
            "@forklaunch/core/services".to_string(),
            vec!["Lifetime", "getEnvVar"],
        ),
        (
            format!("@{app_name}/core"),
            vec!["number", "optional", "string"],
        ),
    ]
}

/// The `{ … }` passed to `const <name> = …(`.
fn config_object(text: &str, class: &[Class], name: &str) -> Result<(usize, usize)> {
    let re = Regex::new(&format!(r"\bconst\s+{name}\s*=")).unwrap();
    let decl = re
        .find_iter(text)
        .find(|m| class[m.start()] == Class::Code)
        .with_context(|| format!("registrations.ts has no `const {name}`"))?;
    let open = (decl.end()..text.len())
        .find(|&i| class[i] == Class::Code && text.as_bytes()[i] == b'{')
        .with_context(|| format!("no object passed to `{name}`"))?;
    let close = matching_close(text, class, open)
        .with_context(|| format!("unbalanced braces in `{name}`"))?;
    Ok((open, close))
}

fn add_entries(
    text: &str,
    name: &str,
    entries: &[(String, String)],
    style: &Style,
) -> Result<String> {
    let mut text = text.to_string();
    for (key, entry) in entries {
        let class = classify(&text);
        let (open, close) = config_object(&text, &class, name)?;
        let items = list_items(&text, &class, open, close);
        if items.iter().any(|i| item_key(&text, i) == *key) {
            continue;
        }
        if items.is_empty() {
            bail!("`{name}` in registrations.ts is empty; add {key} by hand");
        }
        let base_indent = indent_of(&text, items[0].start).to_string();
        let entry = adapt_entry(entry, &text, style, &base_indent);
        let sep = separator(&text, open, &items);
        text = append_item(&text, &items, &entry, &sep);
    }
    Ok(text)
}

fn remove_entries(text: &str, name: &str, keys: &[String]) -> Result<String> {
    let mut text = text.to_string();
    for key in keys {
        let class = classify(&text);
        let Ok((open, close)) = config_object(&text, &class, name) else {
            continue;
        };
        let items = list_items(&text, &class, open, close);
        if let Some(idx) = items.iter().position(|i| item_key(&text, i) == *key) {
            text = remove_item(&text, &items, idx);
        }
    }
    Ok(text)
}

// ---------------------------------------------------------------- imports

#[derive(Debug)]
struct ImportStmt {
    start: usize,
    end: usize,
    source: String,
    is_type: bool,
    /// The `{` of a named import list.
    brace: Option<usize>,
}

fn imports(text: &str) -> Vec<ImportStmt> {
    let re = Regex::new(r#"(?m)^import\s+(type\s+)?([^;'"]*?)\s*from\s*['"]([^'"]+)['"][ \t]*;?"#)
        .unwrap();
    re.captures_iter(text)
        .map(|c| {
            let whole = c.get(0).unwrap();
            let clause = c.get(2).unwrap();
            ImportStmt {
                start: whole.start(),
                end: whole.end(),
                source: c[3].to_string(),
                is_type: c.get(1).is_some(),
                brace: clause.as_str().find('{').map(|i| clause.start() + i),
            }
        })
        .collect()
}

fn import_names(text: &str, stmt: &ImportStmt) -> (Vec<Item>, Vec<String>) {
    let Some(open) = stmt.brace else {
        return (Vec::new(), Vec::new());
    };
    let class = classify(text);
    let Some(close) = matching_close(text, &class, open) else {
        return (Vec::new(), Vec::new());
    };
    let items = list_items(text, &class, open, close);
    let names = items
        .iter()
        .map(|i| local_name(&text[i.start..i.end]))
        .collect();
    (items, names)
}

/// The binding a specifier brings in: `a` for `a`, `type a`, `x as a`.
fn local_name(spec: &str) -> String {
    let spec = spec.strip_prefix("type ").unwrap_or(spec).trim();
    spec.rsplit(" as ")
        .next()
        .unwrap_or(spec)
        .trim()
        .to_string()
}

fn imported_anywhere(text: &str, name: &str) -> bool {
    imports(text)
        .iter()
        .any(|s| import_names(text, s).1.iter().any(|n| n == name))
}

/// Make `names` imported from `source`: into the existing import's list (at
/// its sorted position when it is sorted), else as a new import statement
/// among the package imports.
fn ensure_imports(text: &str, source: &str, names: &[&str], style: &Style) -> String {
    let mut text = text.to_string();
    let missing: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| !imported_anywhere(&text, n))
        .collect();
    if missing.is_empty() {
        return text;
    }
    let existing = imports(&text)
        .into_iter()
        .find(|s| s.source == source && !s.is_type && s.brace.is_some());
    if existing.is_some() {
        for name in missing {
            let stmt = imports(&text)
                .into_iter()
                .find(|s| s.source == source && !s.is_type && s.brace.is_some())
                .unwrap();
            let (items, keys) = import_names(&text, &stmt);
            if items.is_empty() {
                continue;
            }
            let sep = separator(&text, stmt.brace.unwrap(), &items);
            text = match sorted_position(&keys, name) {
                Some(Some(i)) => insert_before(&text, &items[i], name, &sep),
                _ => append_item(&text, &items, name, &sep),
            };
        }
        return text;
    }
    let q = style.quote;
    let line = format!("import {{ {} }} from {q}{source}{q};", missing.join(", "));
    let packages: Vec<ImportStmt> = imports(&text)
        .into_iter()
        .filter(|s| !s.source.starts_with('.'))
        .collect();
    if let Some(next) = packages.iter().find(|s| s.source.as_str() > source) {
        let at = line_start(&text, next.start);
        return format!("{}{line}\n{}", &text[..at], &text[at..]);
    }
    if let Some(last) = packages.last().or(imports(&text).first()) {
        let at = last.end;
        return format!("{}\n{line}{}", &text[..at], &text[at..]);
    }
    format!("{line}\n{text}")
}

/// Drop `names` from `source`'s import (the statement when it empties).
fn drop_imports(text: &str, source: &str, names: &[&str]) -> String {
    let mut text = text.to_string();
    for name in names {
        let Some(stmt) = imports(&text)
            .into_iter()
            .find(|s| s.source == source && !s.is_type && s.brace.is_some())
        else {
            return text;
        };
        let (items, keys) = import_names(&text, &stmt);
        let Some(idx) = keys.iter().position(|k| k == name) else {
            continue;
        };
        if items.len() == 1 {
            let end = if text[stmt.end..].starts_with('\n') {
                stmt.end + 1
            } else {
                stmt.end
            };
            text = format!("{}{}", &text[..stmt.start], &text[end..]);
        } else {
            text = remove_item(&text, &items, idx);
        }
    }
    text
}

fn used_outside_imports(text: &str, name: &str) -> bool {
    let class = classify(text);
    let spans: Vec<(usize, usize)> = imports(text).iter().map(|s| (s.start, s.end)).collect();
    Regex::new(&format!(r"\b{}\b", regex::escape(name)))
        .unwrap()
        .find_iter(text)
        .any(|m| {
            class[m.start()] == Class::Code
                && !spans.iter().any(|(s, e)| m.start() >= *s && m.start() < *e)
        })
}

// ------------------------------------------------------- registrations.ts

pub(crate) fn add_to_registrations(
    text: &str,
    infrastructure: &Infrastructure,
    app_name: &str,
) -> Result<String> {
    let spec = spec(infrastructure);
    let style = detect_style(text);
    let env = template_entries(spec.tag, spec.env_first_key)?;
    let runtime = template_entries(spec.tag, spec.runtime_first_key)?;
    let mut out = add_entries(text, "environmentConfig", &env, &style)?;
    out = add_entries(&out, "runtimeDependencies", &runtime, &style)?;
    let (source, names) = spec.own_import;
    out = ensure_imports(&out, source, names, &style);
    for (source, names) in shared_imports(text, app_name) {
        out = ensure_imports(&out, &source, &names, &style);
    }
    Ok(out)
}

pub(crate) fn remove_from_registrations(
    text: &str,
    infrastructure: &Infrastructure,
    app_name: &str,
) -> Result<String> {
    let spec = spec(infrastructure);
    let keys = |key| -> Result<Vec<String>> {
        Ok(template_entries(spec.tag, key)?
            .into_iter()
            .map(|(k, _)| k)
            .collect())
    };
    let mut out = remove_entries(text, "environmentConfig", &keys(spec.env_first_key)?)?;
    out = remove_entries(&out, "runtimeDependencies", &keys(spec.runtime_first_key)?)?;
    let (source, names) = spec.own_import;
    out = drop_imports(&out, source, names);
    // A shared name goes only when the removed lines were its last use; an
    // import that was already unused is not this command's to drop.
    for (source, names) in shared_imports(text, app_name) {
        let orphaned: Vec<&str> = names
            .into_iter()
            .filter(|n| used_outside_imports(text, n) && !used_outside_imports(&out, n))
            .collect();
        out = drop_imports(&out, &source, &orphaned);
    }
    Ok(out)
}

/// Keys of `runtimeDependencies` that still read `name` (for a warning when
/// the cache is removed from under `AuthCacheService` and the like).
pub(crate) fn still_referenced(text: &str, name: &str) -> bool {
    used_outside_imports(text, name)
}

// ---------------------------------------------------------- test-utils.ts

/// Set `key: true` in the `new BlueprintTestHarness({ … })` options (adding
/// it when missing); on remove, `needsS3` goes and `needsRedis` turns false,
/// as the service template writes them.
pub(crate) fn set_test_harness_flag(text: &str, key: &str, adding: bool) -> Result<String> {
    let class = classify(text);
    let Some(at) = text
        .find("new BlueprintTestHarness(")
        .filter(|i| class[*i] == Class::Code)
    else {
        return Ok(text.to_string());
    };
    let open = (at..text.len())
        .find(|&i| class[i] == Class::Code && text.as_bytes()[i] == b'{')
        .context("BlueprintTestHarness has no options object")?;
    let close = matching_close(text, &class, open).context("unbalanced test harness options")?;
    let items = list_items(text, &class, open, close);
    let found = items.iter().position(|i| item_key(text, i) == key);
    let keep_key = key == "needsRedis";
    Ok(match (found, adding) {
        (Some(idx), _) if keep_key || adding => {
            let item = &items[idx];
            let value = if adding { "true" } else { "false" };
            let current = &text[item.start..item.end];
            let replaced = Regex::new(r"\b(true|false)\b")
                .unwrap()
                .replace(current, value)
                .into_owned();
            format!("{}{replaced}{}", &text[..item.start], &text[item.end..])
        }
        (Some(idx), false) => remove_item(text, &items, idx),
        (None, true) if !items.is_empty() => {
            let sep = separator(text, open, &items);
            append_item(text, &items, &format!("{key}: true"), &sep)
        }
        _ => text.to_string(),
    })
}

// ------------------------------------------------------------ package.json

/// The top-level `"dependencies"` object of a package.json.
fn dependencies_object(text: &str, class: &[Class]) -> Result<(usize, usize)> {
    let root = (0..text.len())
        .find(|&i| class[i] == Class::Code && text.as_bytes()[i] == b'{')
        .context("package.json is not an object")?;
    let root_close = matching_close(text, class, root).context("unbalanced package.json")?;
    let item = list_items(text, class, root, root_close)
        .into_iter()
        .find(|i| item_key(text, i) == "dependencies")
        .context("package.json has no dependencies")?;
    let open = (item.start..item.end)
        .find(|&i| class[i] == Class::Code && text.as_bytes()[i] == b'{')
        .context("dependencies is not an object")?;
    let close = matching_close(text, class, open).context("unbalanced dependencies")?;
    Ok((open, close))
}

/// Add `"name": "version"` to dependencies: alphabetically when the list is
/// sorted, else after the last `@forklaunch/*` entry, else at the end. Key
/// order, formatting and the trailing newline are kept.
pub(crate) fn add_dependency(text: &str, name: &str, version: &str) -> Result<String> {
    let class = classify(text);
    let (open, close) = dependencies_object(text, &class)?;
    let items = list_items(text, &class, open, close);
    let keys: Vec<String> = items.iter().map(|i| item_key(text, i)).collect();
    if keys.iter().any(|k| k == name) {
        return Ok(text.to_string());
    }
    let entry = format!("\"{name}\": \"{version}\"");
    if items.is_empty() {
        let indent = indent_of(text, open);
        return Ok(format!(
            "{}{{\n{indent}  {entry}\n{indent}}}{}",
            &text[..open],
            &text[close + 1..]
        ));
    }
    let sep = separator(text, open, &items);
    let before = match sorted_position(&keys, name) {
        Some(position) => position,
        None => keys
            .iter()
            .rposition(|k| k.starts_with("@forklaunch/"))
            .map(|i| i + 1)
            .filter(|i| *i < items.len()),
    };
    Ok(match before {
        Some(i) => insert_before(text, &items[i], &entry, &sep),
        None => append_item(text, &items, &entry, &sep),
    })
}

pub(crate) fn remove_dependency(text: &str, name: &str) -> Result<String> {
    let class = classify(text);
    let (open, close) = dependencies_object(text, &class)?;
    let items = list_items(text, &class, open, close);
    Ok(match items.iter().position(|i| item_key(text, i) == name) {
        Some(idx) => remove_item(text, &items, idx),
        None => text.to_string(),
    })
}

// ----------------------------------------------------------- manifest.toml

/// Set (or with `None`, remove) `key` in the service's `[projects.resources]`,
/// leaving the rest of the manifest as it is.
pub(crate) fn set_manifest_resource(
    text: &str,
    service: &str,
    key: &str,
    value: Option<&str>,
) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("parsing manifest.toml")?;
    let projects = doc
        .get_mut("projects")
        .and_then(|p| p.as_array_of_tables_mut())
        .context("manifest has no [[projects]]")?;
    let project = projects
        .iter_mut()
        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(service))
        .with_context(|| format!("no project named '{service}' in the manifest"))?;
    if !project.contains_key("resources") {
        if value.is_none() {
            return Ok(text.to_string());
        }
        let mut resources = toml_edit::Table::new();
        resources.set_position(project.position().map(|p| p + 1));
        project.insert("resources", toml_edit::Item::Table(resources));
    }
    let resources = project
        .get_mut("resources")
        .and_then(|r| r.as_table_like_mut())
        .context("resources is not a table")?;
    match value {
        Some(v) => {
            resources.insert(key, toml_edit::value(v));
        }
        None => {
            resources.remove(key);
        }
    }
    Ok(doc.to_string())
}

// -------------------------------------------------------------- .env.local

pub(crate) fn remove_env_lines(text: &str, keys: &[&str]) -> String {
    text.split_inclusive('\n')
        .filter(|line| {
            let line = line.trim_start();
            !keys.iter().any(|k| line.starts_with(&format!("{k}=")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A customized registrations.ts in the style biome writes for a real app:
    /// a header doc comment, inline comments, two-space indentation, single
    /// quotes, no trailing commas, custom registrations.
    const REGISTRATIONS: &str = r#"/**
 * Vault service registrations.
 * Feature-specific DI is appended below the runtime base.
 */

import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FieldEncryptor,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import {
  ComplianceDataService,
  Lifetime,
  createConfigInjector,
  getEnvVar
} from '@forklaunch/core/services';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { number, optional, schemaValidator, string } from '@hv/core';
import { metrics } from '@hv/monitoring';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import { FactsService } from './domain/services/facts.service';
import mikroOrmOptionsConfig from './mikro-orm.config';

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'vault', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
  REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  },
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('OTEL_LEVEL')
  },
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
  },
  // Epic SMART/PKCE OAuth. Optional until a registration exists.
  EPIC_CLIENT_ID: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('EPIC_CLIENT_ID')
  }
});

const runtimeDependencies = environmentConfig.chain({
  Orm: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_LEVEL }) =>
      new OpenTelemetryCollector('vault', OTEL_LEVEL || 'info', metrics)
  },
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OtelCollector, OTEL_LEVEL, ENCRYPTION_KEY }) =>
      new RedisTtlCache(
        60 * 60 * 1000,
        OtelCollector,
        { url: REDIS_URL },
        { enabled: true, level: OTEL_LEVEL || 'info' },
        { encryptor: new FieldEncryptor(ENCRYPTION_KEY) }
      )
  },
  // Tenant-scoped entity manager.
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ Orm }, context?: { tenantId?: string }) =>
      wrapEmWithTenantContext(Orm.em.fork(), context?.tenantId) as EntityManager
  }
});

const serviceDependencies = runtimeDependencies.chain({
  ComplianceDataService: {
    lifetime: Lifetime.Singleton,
    type: ComplianceDataService,
    factory: ({ Orm, OtelCollector }) =>
      new ComplianceDataService(Orm, OtelCollector)
  },
  FactsService: {
    lifetime: Lifetime.Scoped,
    type: FactsService,
    factory: ({ EntityManager, OtelCollector }) =>
      new FactsService(EntityManager, OtelCollector)
  }
});

export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
"#;

    fn removed_lines(before: &str, after: &str) -> Vec<String> {
        let after_lines: Vec<&str> = after.lines().collect();
        before
            .lines()
            .filter(|l| !after_lines.contains(l))
            .map(String::from)
            .collect()
    }

    #[test]
    fn object_store_add_keeps_every_existing_line_and_remove_round_trips() {
        let added = add_to_registrations(REGISTRATIONS, &Infrastructure::S3, "hv").unwrap();
        // Nothing the file had is touched: every original line is still there,
        // in order.
        let mut rest = added.as_str();
        for line in REGISTRATIONS.lines() {
            let at = rest
                .find(line)
                .unwrap_or_else(|| panic!("lost or reordered line: {line:?}\n{added}"));
            rest = &rest[at + line.len()..];
        }
        assert!(removed_lines(REGISTRATIONS, &added).is_empty());
        for comment in [
            " * Vault service registrations.",
            "  // Epic SMART/PKCE OAuth. Optional until a registration exists.",
            "  // Tenant-scoped entity manager.",
        ] {
            assert!(added.contains(comment), "comment dropped: {comment}");
        }
        assert!(added.contains(
            "import { RedisTtlCache } from '@forklaunch/infrastructure-redis';\nimport { S3ObjectStore, s3ClientConfig } from '@forklaunch/infrastructure-s3';\nimport { number, optional, schemaValidator, string } from '@hv/core';"
        ));
        // The keyless template registration, in the file's style.
        assert!(added.contains("  S3_ACCESS_KEY_ID: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('S3_ACCESS_KEY_ID')\n  },"));
        assert!(
            added.contains("  S3_BUCKET: {\n    lifetime: Lifetime.Singleton,\n    type: string,")
        );
        assert!(added.contains(
            "    value: Number(getEnvVar('S3_PRESIGN_MAX_DOWNLOAD_SECONDS')) || undefined\n  }\n});"
        ));
        assert!(added.contains(
            "  ObjectStore: {\n    lifetime: Lifetime.Singleton,\n    type: S3ObjectStore,"
        ));
        assert!(added.contains(
            "          // Deployed on ForkLaunch only the region is set: credentials come\n"
        ));
        assert!(
            added.contains("          clientConfig: s3ClientConfig({\n            url: S3_URL,")
        );
        assert!(added.contains(
            "          encryptor: new FieldEncryptor(ENCRYPTION_KEY)\n        }\n      )\n  }\n});"
        ));
        assert!(!added.contains("LEGACY_ENCRYPTION_KEYS"));
        assert!(!added.contains('"'), "switched quote style");
        assert!(!added.contains('\t'), "switched indentation");

        let removed = remove_from_registrations(&added, &Infrastructure::S3, "hv").unwrap();
        assert_eq!(removed, REGISTRATIONS);
    }

    #[test]
    fn cache_remove_then_add_round_trips_and_keeps_shared_imports() {
        let removed =
            remove_from_registrations(REGISTRATIONS, &Infrastructure::Redis, "hv").unwrap();
        assert!(!removed.contains("RedisTtlCache"));
        assert!(!removed.contains("REDIS_URL"));
        // FieldEncryptor was only the cache's; the other import stays.
        assert!(removed.contains(
            "import {\n  wrapEmWithTenantContext\n} from '@forklaunch/core/persistence';"
        ));
        assert!(removed.contains("  // Tenant-scoped entity manager.\n"));
        let added = add_to_registrations(&removed, &Infrastructure::Redis, "hv").unwrap();
        assert!(
            added.contains("import { RedisTtlCache } from '@forklaunch/infrastructure-redis';")
        );
        assert!(added.contains(
            "  TtlCache: {\n    lifetime: Lifetime.Singleton,\n    type: RedisTtlCache,"
        ));
        assert!(
            added.contains("level: 'info'"),
            "double quotes kept:\n{added}"
        );
        assert_eq!(
            remove_from_registrations(&added, &Infrastructure::Redis, "hv").unwrap(),
            removed
        );
    }

    #[test]
    fn missing_shared_imports_are_added_and_removed_again() {
        let file = REGISTRATIONS
            .replace(
                "import { number, optional, schemaValidator, string } from '@hv/core';",
                "import { schemaValidator, string } from '@hv/core';",
            )
            .replace("    type: number,\n", "    type: string,\n")
            .replace("    type: optional(string),\n", "    type: string,\n")
            .replace("  FieldEncryptor,\n", "")
            .replace(
                "        { encryptor: new FieldEncryptor(ENCRYPTION_KEY) }\n",
                "        {}\n",
            );
        let added = add_to_registrations(&file, &Infrastructure::S3, "hv").unwrap();
        assert!(
            added.contains("import { number, optional, schemaValidator, string } from '@hv/core';")
        );
        assert!(added.contains("import {\n  FieldEncryptor,\n  wrapEmWithTenantContext\n} from '@forklaunch/core/persistence';"));
        assert_eq!(
            remove_from_registrations(&added, &Infrastructure::S3, "hv").unwrap(),
            file
        );
    }

    #[test]
    fn template_style_files_keep_tabs_double_quotes_and_legacy_keys() {
        let file = "import { string, optional, SchemaValidator } from \"@demo/core\";\nimport { OpenTelemetryCollector } from \"@forklaunch/core/http\";\nimport { createConfigInjector, getEnvVar, Lifetime } from \"@forklaunch/core/services\";\n\nconst environmentConfig = configInjector.chain({\n\tENCRYPTION_KEY: {\n\t\tlifetime: Lifetime.Singleton,\n\t\ttype: string,\n\t\tvalue: getEnvVar(\"ENCRYPTION_KEY\"),\n\t},\n\tLEGACY_ENCRYPTION_KEYS: {\n\t\tlifetime: Lifetime.Singleton,\n\t\ttype: optional(string),\n\t\tvalue: getEnvVar(\"LEGACY_ENCRYPTION_KEYS\"),\n\t},\n});\n\nconst runtimeDependencies = environmentConfig.chain({\n\tOtelCollector: {\n\t\tlifetime: Lifetime.Singleton,\n\t\ttype: OpenTelemetryCollector,\n\t\tfactory: () => new OpenTelemetryCollector(\"x\", \"info\"),\n\t},\n});\n";
        let added = add_to_registrations(file, &Infrastructure::S3, "demo").unwrap();
        assert!(added.contains("\tS3_REGION: {\n\t\tlifetime: Lifetime.Singleton,\n\t\ttype: optional(string),\n\t\tvalue: getEnvVar(\"S3_REGION\")\n\t},"));
        assert!(added.contains("previousKeys: parseEncryptionKeyList(LEGACY_ENCRYPTION_KEYS)"));
        assert!(added.contains(
            "import { S3ObjectStore, s3ClientConfig } from \"@forklaunch/infrastructure-s3\";"
        ));
        assert!(
            added.contains(
                "import { string, optional, SchemaValidator, number } from \"@demo/core\";"
            )
        );
        assert!(added.contains("import { OpenTelemetryCollector } from \"@forklaunch/core/http\";\nimport { FieldEncryptor, parseEncryptionKeyList } from \"@forklaunch/core/persistence\";\nimport { createConfigInjector"));
        assert_eq!(
            remove_from_registrations(&added, &Infrastructure::S3, "demo").unwrap(),
            file
        );
    }

    const PACKAGE_JSON: &str = r#"{
  "name": "@demo/vault",
  "scripts": {
    "zeta": "run zeta",
    "alpha": "run alpha"
  },
  "dependencies": {
    "@better-auth/passkey": "1.7.1",
    "@forklaunch/core": "~1.5.15",
    "@forklaunch/infrastructure-redis": "~1.4.10",
    "@forklaunch/interfaces-iam": "~1.0.30",
    "@health-vault/core": "workspace:*",
    "ioredis": "^6.0.0",
    "zod": "^4.4.3"
  },
  "mikro-orm": {
    "manifest_paths": ["./mikro-orm.config.ts", "./dist/mikro-orm.config.js"]
  }
}
"#;

    #[test]
    fn package_json_gets_one_sorted_line_and_round_trips() {
        let added =
            add_dependency(PACKAGE_JSON, "@forklaunch/infrastructure-s3", "~1.5.1").unwrap();
        assert_eq!(
            added,
            PACKAGE_JSON.replace(
                "    \"@forklaunch/infrastructure-redis\": \"~1.4.10\",\n",
                "    \"@forklaunch/infrastructure-redis\": \"~1.4.10\",\n    \"@forklaunch/infrastructure-s3\": \"~1.5.1\",\n"
            )
        );
        assert_eq!(
            remove_dependency(&added, "@forklaunch/infrastructure-s3").unwrap(),
            PACKAGE_JSON
        );
        // Removing the last entry and adding it back.
        let without_zod = remove_dependency(PACKAGE_JSON, "zod").unwrap();
        assert!(without_zod.contains("\"ioredis\": \"^6.0.0\"\n  },"));
        assert_eq!(
            add_dependency(&without_zod, "zod", "^4.4.3").unwrap(),
            PACKAGE_JSON
        );
    }

    #[test]
    fn unsorted_package_json_gets_the_line_after_the_last_forklaunch_entry() {
        let unsorted = PACKAGE_JSON
            .replace("    \"@better-auth/passkey\": \"1.7.1\",\n", "")
            .replace(
                "    \"zod\": \"^4.4.3\"\n",
                "    \"zod\": \"^4.4.3\",\n    \"@better-auth/passkey\": \"1.7.1\"\n",
            );
        let added = add_dependency(&unsorted, "@forklaunch/infrastructure-s3", "~1.5.1").unwrap();
        assert!(added.contains("    \"@forklaunch/interfaces-iam\": \"~1.0.30\",\n    \"@forklaunch/infrastructure-s3\": \"~1.5.1\",\n    \"@health-vault/core\""));
        assert_eq!(
            remove_dependency(&added, "@forklaunch/infrastructure-s3").unwrap(),
            unsorted
        );
    }

    const MANIFEST: &str = r#"id = "f0400ba0"
cli_version = "1.21.0"
app_name = "demo"
modules_path = "src/modules"

# The projects.
[[projects]]
type = "service"
name = "vault"
description = "Vault"
routers = [
  "facts",
]

[projects.resources]
database = "postgresql"
cache = "redis"

[[projects]]
type = "service"
name = "iam"
description = "IAM"

[project_peer_topology]
demo = ["vault", "iam"]
"#;

    #[test]
    fn manifest_gets_only_the_resource_key() {
        let added = set_manifest_resource(MANIFEST, "vault", "object_store", Some("s3")).unwrap();
        assert_eq!(
            added,
            MANIFEST.replace(
                "cache = \"redis\"\n",
                "cache = \"redis\"\nobject_store = \"s3\"\n"
            )
        );
        assert!(!added.contains("snake_case_name"));
        assert!(!added.contains("Generated by ForkLaunch"));
        assert_eq!(
            set_manifest_resource(&added, "vault", "object_store", None).unwrap(),
            MANIFEST
        );
    }

    #[test]
    fn manifest_resources_table_is_created_under_its_project() {
        let added = set_manifest_resource(MANIFEST, "iam", "cache", Some("redis")).unwrap();
        let parsed: toml::Value = toml::from_str(&added).unwrap();
        let iam = &parsed["projects"].as_array().unwrap()[1];
        assert_eq!(iam["name"].as_str(), Some("iam"));
        assert_eq!(iam["resources"]["cache"].as_str(), Some("redis"));
        assert!(added.starts_with(MANIFEST.split("[project_peer_topology]").next().unwrap()));
        assert!(added.contains("[project_peer_topology]\ndemo = [\"vault\", \"iam\"]\n"));
    }

    #[test]
    fn test_harness_flags() {
        let text = "export const setup = async () => {\n  harness = new BlueprintTestHarness({\n    useMigrations: true,\n    needsRedis: false\n  });\n};\n";
        let with_s3 = set_test_harness_flag(text, "needsS3", true).unwrap();
        assert!(with_s3.contains("    needsRedis: false,\n    needsS3: true\n  });"));
        assert_eq!(
            set_test_harness_flag(&with_s3, "needsS3", false).unwrap(),
            text
        );
        let with_redis = set_test_harness_flag(text, "needsRedis", true).unwrap();
        assert!(with_redis.contains("needsRedis: true\n"));
        assert_eq!(
            set_test_harness_flag(&with_redis, "needsRedis", false).unwrap(),
            text
        );
    }

    #[test]
    fn env_lines_are_removed_by_key() {
        assert_eq!(
            remove_env_lines("A=1\nS3_URL=x\nS3_URL_EXTRA=y\n", &["S3_URL"]),
            "A=1\nS3_URL_EXTRA=y\n"
        );
    }
}
