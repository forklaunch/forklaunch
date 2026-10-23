use std::path::{Path, PathBuf};

pub(crate) fn get_relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(from, to)| from == to)
        .count();
    let mut result = PathBuf::new();
    for _ in common..from_components.len() {
        result.push("..");
    }
    for comp in &to_components[common..] {
        result.push(comp);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_paths_retain_divergent_and_ancestor_components() {
        for (from, to, expected) in [
            (
                "/app/crates/storage",
                "/app/.forklaunch/framework",
                "../../.forklaunch/framework",
            ),
            (
                "/app/services/team/storage",
                "/app/.forklaunch/framework",
                "../../../.forklaunch/framework",
            ),
            ("/app/modules/service", "/app", "../.."),
            ("/app", "/app/modules/service", "modules/service"),
            ("/app", "/app", ""),
            (
                "crates/service",
                ".forklaunch/framework",
                "../../.forklaunch/framework",
            ),
        ] {
            assert_eq!(
                get_relative_path(Path::new(from), Path::new(to)),
                PathBuf::from(expected)
            );
        }
    }
}
