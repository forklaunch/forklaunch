//! Cargo's locked registry releases use OSV's case-sensitive `crates.io` ecosystem.
use super::*;

#[derive(Deserialize)]
struct Lockfile {
    version: Option<u64>,
    package: Option<Vec<LockedPackage>>,
}

#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
}

pub(super) fn collect(
    root: &Path,
    lockfiles: &mut BTreeSet<String>,
    unreadable: &mut Vec<String>,
    unsupported: &mut Vec<String>,
    add: &mut impl FnMut(&'static str, String, String, &Path),
) {
    for file in walk(root, &["Cargo.lock"]) {
        let relative = rel(root, &file);
        lockfiles.insert(relative.clone());
        let parsed = fs::read_to_string(&file)
            .ok()
            .and_then(|text| toml::from_str::<Lockfile>(&text).ok());
        let Some(lock) = parsed else {
            unreadable.push(relative);
            continue;
        };
        // Version 1 omitted the version field. Later versions retain package records.
        // A different TOML document must never be reported as an empty, clean lockfile.
        if (lock.package.is_none() && lock.version.is_none())
            || lock
                .version
                .is_some_and(|version| !(1..=4).contains(&version))
            || lock.package.as_ref().is_some_and(|packages| {
                packages
                    .iter()
                    .any(|package| package.name.is_empty() || !is_version(&package.version))
            })
        {
            unreadable.push(relative);
            continue;
        }
        for package in lock.package.unwrap_or_default() {
            match package.source.as_deref() {
                // Workspace and path packages are the app's own source, not registry releases.
                None => {}
                Some("registry+https://github.com/rust-lang/crates.io-index") => {
                    add("crates.io", package.name, package.version, &file);
                }
                // Do not disclose a source URL: private registries can embed credentials.
                Some(_) => {
                    unsupported.push(format!("{}@{} ({relative})", package.name, package.version))
                }
            }
        }
    }
    unsupported.sort();
    unsupported.dedup();
}
