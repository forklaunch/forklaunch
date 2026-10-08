use std::{ffi::OsString, fs, io, path::Path, process::Command};

/// Configure customer package code with a new, credential-free home and environment.
/// `home` must be a fresh directory owned by this command, never a project-selected path.
pub fn isolate_package_environment(command: &mut Command, home: &Path) -> io::Result<()> {
    configure(command, home, std::env::vars_os())
}

fn configure(
    command: &mut Command,
    home: &Path,
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) -> io::Result<()> {
    for dir in ["tmp", ".config", ".cache"] {
        fs::create_dir(home.join(dir))?;
    }
    // Never follow or overwrite a configuration path left by customer code.
    for file in [".npmrc", ".gitconfig"] {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(home.join(file))?;
    }
    command.env_clear();
    for (key, value) in inherited {
        if matches!(
            key.to_str(),
            Some(
                "PATH"
                    | "Path"
                    | "SYSTEMROOT"
                    | "SystemRoot"
                    | "WINDIR"
                    | "PATHEXT"
                    | "COMSPEC"
                    | "ComSpec"
            )
        ) {
            command.env(key, value);
        }
    }
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("TMPDIR", home.join("tmp"))
        .env("TEMP", home.join("tmp"))
        .env("TMP", home.join("tmp"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("npm_config_userconfig", home.join(".npmrc"))
        .env("npm_config_globalconfig", home.join(".npmrc"))
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .env("COREPACK_ENABLE_DOWNLOAD_PROMPT", "0")
        .env("CI", "true")
        .env("TERM", "dumb");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn home() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "fl-package-env-{}-{}-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn strips_credentials_and_execution_hooks() {
        let home = home();
        let mut command = Command::new("unused");
        configure(
            &mut command,
            &home,
            [
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("FORKLAUNCH_HMAC_SECRET".into(), "synthetic".into()),
                ("AWS_SECRET_ACCESS_KEY".into(), "synthetic".into()),
                ("NODE_OPTIONS".into(), "--unsafe-hook".into()),
                ("NPM_TOKEN".into(), "synthetic".into()),
                ("HOME".into(), "/operator-home".into()),
            ],
        )
        .unwrap();
        let vars: std::collections::HashMap<_, _> = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.unwrap().to_string_lossy().to_string(),
                )
            })
            .collect();
        for key in [
            "FORKLAUNCH_HMAC_SECRET",
            "AWS_SECRET_ACCESS_KEY",
            "NODE_OPTIONS",
            "NPM_TOKEN",
        ] {
            assert!(!vars.contains_key(key));
        }
        assert_eq!(vars["HOME"], home.to_string_lossy());
        fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn refuses_existing_configuration_instead_of_overwriting_it() {
        let home = home();
        fs::write(home.join(".npmrc"), "keep").unwrap();
        assert!(configure(&mut Command::new("unused"), &home, []).is_err());
        assert_eq!(fs::read_to_string(home.join(".npmrc")).unwrap(), "keep");
        fs::remove_dir_all(home).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn real_child_uses_empty_home_without_parent_credentials() {
        let home = home();
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("test -z \"$FORKLAUNCH_HMAC_SECRET$AWS_SECRET_ACCESS_KEY$NODE_OPTIONS$NPM_TOKEN\" && test -f \"$HOME/.npmrc\" && test ! -e \"$HOME/.codex/auth.json\"");
        isolate_package_environment(&mut command, &home).unwrap();
        assert!(command.status().unwrap().success());
        fs::remove_dir_all(home).unwrap();
    }
}
