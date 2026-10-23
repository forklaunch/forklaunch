//! Platform-held capabilities a service reaches through the managed gateway
//! (the instance gateway): payments, email, sms, whatsapp, voice, models. Any
//! ForkLaunch app can hold them, a managed instance or an app deployed to its
//! own environment.
//!
//! `forklaunch infra add <service> <capability>` records the capability in the
//! manifest (`resources.capabilities`, which the release manifest turns into a
//! platform resource bound to the service) and applies the capability's own
//! edits: its registration in `registrations.ts`, the gateway settings the
//! service reads, the local gateway mock in docker-compose, and — for
//! capabilities the platform sends events for — the `/platform-events` route.
//! `infra remove` undoes them.
//!
//! Each capability is one file in this directory exporting a `Capability`,
//! listed in `CAPABILITIES`. The shared edits live on `CapabilityEdit`, so a
//! capability's file only says what is particular to it.

mod native;
pub(crate) use native::{cleanup_deleted_native_service, compose_processes, rename_native_capabilities};
pub(crate) use native::{
    dependency_in_use, ensure_dependency as ensure_native_dependency, release_dependency,
};

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

pub(crate) mod email;
pub(crate) mod sms;
pub(crate) mod whatsapp;
pub(crate) mod voice;
pub(crate) mod payments;
pub(crate) mod models;

use super::in_place;
use crate::core::docker::{DependencyCondition, DependsOn, DockerCompose, DockerService};

/// One platform-held capability.
pub(crate) struct Capability {
    /// The name users type: `payments`, `email`, …
    pub(crate) id: &'static str,
    /// The platform's IntegrationType literal for its release-manifest resource.
    pub(crate) resource_type: &'static str,
    /// One line for `--help` and errors.
    pub(crate) summary: &'static str,
    /// The `runtimeDependencies` key the capability registers. The wiring check
    /// looks for `<key>:` in registrations.ts.
    pub(crate) registration_key: &'static str,
    /// Whether the platform delivers events for it (adds `/platform-events`).
    pub(crate) receives_events: bool,
    /// Whether a worker may hold it (`infra add <worker> <capability>`). A
    /// worker can call the gateway (send, place a call, charge, call a
    /// model); events are still delivered over HTTP to `/platform-events`,
    /// which the worker process does not serve.
    pub(crate) workers: bool,
    /// Capability-specific edits on add; the shared ones are applied around it.
    pub(crate) add: fn(&mut CapabilityEdit) -> Result<()>,
    /// Capability-specific edits on remove.
    pub(crate) remove: fn(&mut CapabilityEdit) -> Result<()>,
}

/// Every capability `infra add` knows. Each feature adds its entry.
pub(crate) static CAPABILITIES: &[&Capability] = &[
    &email::EMAIL,
    &sms::SMS,
    &whatsapp::WHATSAPP,
    &voice::VOICE,
    &payments::PAYMENTS,
    &models::MODELS,
];

/// The code edits of `infra add|remove <service> <capability>`: the
/// capability's own, then the shared gateway wiring (on remove in reverse,
/// so remove takes back exactly what add wrote). `edit` carries the
/// capabilities as they are after the change.
pub(crate) fn apply(edit: &mut CapabilityEdit, capability: &Capability, adding: bool) -> Result<()> {
    if edit.native {
        return native::apply(edit, capability, adding);
    }
    if adding {
        (capability.add)(edit)?;
        edit.ensure_gateway_wiring(capability.id)
    } else {
        (capability.remove)(edit)?;
        if capability.receives_events {
            edit.remove_platform_events(capability.id)?;
        }
        edit.release_gateway_wiring(capability.id)
    }
}

pub(crate) fn find(id: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().copied().find(|c| c.id == id)
}

/// The platform resource type for a capability id (the id itself if unknown,
/// so a newer manifest never loses a capability an older CLI doesn't know).
pub(crate) fn resource_type(id: &str) -> &str {
    find(id).map(|c| c.resource_type).unwrap_or(id)
}

/// The key the local gateway mock and a local service share.
pub(crate) const LOCAL_INSTANCE_HMAC_KEY: &str = "local-dev-instance-key";
pub(crate) const LOCAL_INSTANCE_ID: &str = "local-instance";
pub(crate) const GATEWAY_MOCK_SERVICE: &str = "gateway-mock";
/// Host port for the mock's control endpoints (`/__mock/requests`, …).
pub(crate) const GATEWAY_MOCK_HOST_PORT: &str = "18088";

/// Staged file edits for one `infra add|remove`, written together at the end
/// (or printed with `--dryrun`).
pub(crate) struct CapabilityEdit {
    pub(crate) native: bool,
    /// Format staged Rust when adding; removal restores the user's original bytes.
    pub(crate) format_rust: bool,
    pub(crate) app_root: PathBuf,
    pub(crate) app_name: String,
    pub(crate) service_name: String,
    pub(crate) service_path: PathBuf,
    /// Capabilities of this service after the change.
    pub(crate) service_capabilities: Vec<String>,
    /// Capabilities of every project after the change.
    pub(crate) app_capabilities: Vec<String>,
    /// The docker-compose services that run this project's code. A service
    /// is one (`<name>`); a worker is two (`<name>-worker`, `<name>-server`).
    /// The last one receives the mock's events.
    compose_keys: Vec<String>,
    staged: BTreeMap<PathBuf, Option<String>>,
}

impl CapabilityEdit {
    pub(crate) fn new(
        app_root: &Path,
        app_name: &str,
        service_name: &str,
        service_path: &Path,
        service_capabilities: Vec<String>,
        app_capabilities: Vec<String>,
    ) -> Self {
        Self {
            native: false,
            format_rust: false,
            app_root: app_root.to_path_buf(),
            app_name: app_name.to_string(),
            service_name: service_name.to_string(),
            service_path: service_path.to_path_buf(),
            service_capabilities,
            app_capabilities,
            compose_keys: vec![service_name.to_string()],
            staged: BTreeMap::new(),
        }
    }

    /// For a worker project: its worker and server containers both get the
    /// gateway settings; the server (which mounts `/platform-events`)
    /// receives the mock's events.
    pub(crate) fn for_worker(mut self) -> Self {
        self.compose_keys = vec![
            format!("{}-worker", self.service_name),
            format!("{}-server", self.service_name),
        ];
        self
    }

    /// A file's content as it will be after the staged edits.
    pub(crate) fn read(&self, path: &Path) -> Result<Option<String>> {
        if let Some(staged) = self.staged.get(path) {
            return Ok(staged.clone());
        }
        if path.exists() {
            return Ok(Some(
                fs::read_to_string(path).with_context(|| format!("reading {path:?}"))?,
            ));
        }
        Ok(None)
    }

    /// Rust sources are staged in the application's rustfmt style, so its CI
    /// format check passes on code capabilities add.
    pub(crate) fn write(&mut self, path: PathBuf, content: String) {
        let content = if self.format_rust && path.extension().is_some_and(|extension| extension == "rs") {
            crate::init::rust_project::format_rust_source(&path, &content)
        } else {
            content
        };
        self.staged.insert(path, Some(content));
    }

    pub(crate) fn delete(&mut self, path: PathBuf) {
        self.staged.insert(path, None);
    }

    pub(crate) fn registrations_path(&self) -> PathBuf {
        self.service_path.join("registrations.ts")
    }

    /// Paths this edit would change, for `--dryrun`.
    pub(crate) fn changed_paths(&self) -> Vec<(PathBuf, bool)> {
        self.staged
            .iter()
            .map(|(path, content)| (path.clone(), content.is_some()))
            .collect()
    }

    pub(crate) fn commit(self) -> Result<()> {
        for (path, content) in self.staged {
            match content {
                Some(content) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&path, content).with_context(|| format!("writing {path:?}"))?;
                }
                None => {
                    if path.exists() {
                        fs::remove_file(&path).with_context(|| format!("removing {path:?}"))?;
                        // A directory add created for the file goes with it.
                        if let Some(parent) = path.parent()
                            && fs::read_dir(parent).is_ok_and(|mut entries| entries.next().is_none())
                        {
                            fs::remove_dir(parent)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------- stateless removal

    /// Whether `text` is what add writes to `path` from `template` (staged
    /// Rust is formatted, and either form counts).
    pub(crate) fn is_generated(&self, path: &Path, text: &str, template: &str) -> bool {
        text == template
            || (path.extension().is_some_and(|extension| extension == "rs")
                && text == crate::init::rust_project::format_rust_source(path, template))
    }

    fn display<'a>(&self, path: &'a Path) -> std::path::Display<'a> {
        path.strip_prefix(&self.app_root).unwrap_or(path).display()
    }

    /// Removal keeps a file edited since add wrote it.
    pub(crate) fn note_kept(&self, path: &Path) {
        eprintln!("note: kept {} (edited since `infra add`)", self.display(path));
    }

    /// Removal keeps a setting `what` in `path` that no longer holds the value add wrote.
    pub(crate) fn note_kept_setting(&self, path: &Path, what: &str) {
        eprintln!(
            "note: kept {what} in {} (changed since `infra add`)",
            self.display(path)
        );
    }

    /// Undo [`Self::ensure_env_local`]: drop the `KEY=value` lines that still
    /// hold the value add writes, and the file once nothing is left. A key set
    /// to anything else is the developer's and stays.
    pub(crate) fn release_env_local(&mut self, pairs: &[(&str, &str)]) -> Result<()> {
        let path = self.service_path.join(".env.local");
        let Some(text) = self.read(&path)? else {
            return Ok(());
        };
        let kept = in_place::remove_env_pairs(&text, pairs);
        // Name kept settings from the file itself, never from the pairs (whose
        // values may be secrets).
        let kept_names: Vec<String> = kept
            .lines()
            .filter_map(in_place::env_key)
            .filter(|name| pairs.iter().any(|(added, _)| added == name))
            .map(str::to_owned)
            .collect();
        for name in &kept_names {
            self.note_kept_setting(&path, name);
        }
        if kept == text {
            return Ok(());
        }
        if kept.trim().is_empty() {
            self.delete(path);
        } else {
            self.write(path, kept);
        }
        Ok(())
    }

    // ---------------------------------------------------------------- code

    /// Add an import and config-injector entries to registrations.ts, as
    /// text in the file's own style (see `in_place`): nothing else in the
    /// file moves.
    ///
    /// `import_line` is a one-line `import { a, type B } from '…';` whose
    /// names are merged into the file's import from `import_source` (or a
    /// default `import X from '…';`, added unless present); empty for none.
    /// `env_block` and `runtime_block` are complete
    /// `const configInjector = createConfigInjector(SchemaValidator(), { … });`
    /// snippets whose entries are appended to `environmentConfig` and
    /// `runtimeDependencies`. Entries already present are left alone.
    pub(crate) fn inject_registration(
        &mut self,
        import_line: &str,
        import_source: &str,
        env_block: Option<&str>,
        runtime_block: Option<&str>,
    ) -> Result<()> {
        let path = self.registrations_path();
        let before = self
            .read(&path)?
            .with_context(|| format!("{path:?} not found"))?;
        let mut text = before.clone();
        let names = named_imports(import_line);
        if !names.is_empty() {
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            text = in_place::add_named_imports(&text, import_source, &names);
        } else if let Some(name) = default_import(import_line) {
            text = in_place::add_default_import(&text, &name, import_source);
        }
        for (block, declaration) in [
            (env_block, "environmentConfig"),
            (runtime_block, "runtimeDependencies"),
        ] {
            if let Some(block) = block {
                text = in_place::add_config_entries(&text, declaration, block)?;
            }
        }
        if text != before {
            self.write(path, text);
        }
        Ok(())
    }

    /// Remove config-injector entries, then the imports only they read (the
    /// ones `inject_registration` added), and with `import_source` that
    /// whole import.
    pub(crate) fn remove_registration(
        &mut self,
        import_source: Option<&str>,
        env_keys: &[&str],
        runtime_keys: &[&str],
    ) -> Result<()> {
        let path = self.registrations_path();
        let Some(before) = self.read(&path)? else {
            return Ok(());
        };
        let mut text = in_place::remove_config_entries(&before, "environmentConfig", env_keys)?;
        text = in_place::remove_config_entries(&text, "runtimeDependencies", runtime_keys)?;
        text = in_place::drop_orphaned_imports(&before, &text, ADDED_IMPORTS);
        if let Some(source) = import_source {
            if let Some((_, _, names)) = find_import(&text, source) {
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                text = in_place::remove_named_imports(&text, source, &names);
            }
        }
        if text != before {
            self.write(path, text);
        }
        Ok(())
    }

    // ---------------------------------------------------- gateway settings

    /// The service reads the gateway contract (`PLATFORM_GATEWAY_URL`,
    /// `INSTANCE_ID`, `INSTANCE_HMAC_KEY`, all optional: absent outside managed
    /// mode), and locally talks to the gateway mock in docker-compose, which
    /// delivers `feature` events back to it.
    pub(crate) fn ensure_gateway_wiring(&mut self, feature: &str) -> Result<()> {
        let registrations = self.read(&self.registrations_path())?.unwrap_or_default();
        if !registrations.contains("INSTANCE_HMAC_KEY:") {
            self.inject_registration(
                "import { getEnvVar } from \"@forklaunch/core/services\";",
                "@forklaunch/core/services",
                Some(GATEWAY_ENV_BLOCK),
                None,
            )?;
        }
        let pairs = local_gateway_env();
        let pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.ensure_env_local(&pairs)?;
        let keys = self.compose_keys.clone();
        self.update_compose(|compose| {
            // Every container that runs this project's code must be there (a
            // worker is two): wiring only one would leave the other without
            // the gateway settings, or send the events to the wrong one.
            let missing: Vec<&str> = keys
                .iter()
                .map(String::as_str)
                .filter(|k| !compose.services.contains_key(*k))
                .collect();
            if !missing.is_empty() {
                bail!(
                    "no {} service in docker-compose (needs: {})",
                    missing
                        .iter()
                        .map(|k| format!("'{k}'"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    keys.join(", ")
                );
            }
            // The last one (a worker's server) mounts /platform-events, for a
            // capability the platform sends events for.
            let events = find(feature).is_none_or(|c| c.receives_events);
            for (i, key) in keys.iter().enumerate() {
                ensure_gateway_mock(compose, key, feature, events && i + 1 == keys.len())?;
            }
            Ok(())
        })
    }

    /// Undo `ensure_gateway_wiring` for `feature`, dropping the service's
    /// gateway settings when it has no capability left, and the mock when
    /// the app has none. Settings the developer wrote (a registration that is
    /// not the one `infra add` writes, or one other code still reads; an env
    /// value of their own) stay.
    pub(crate) fn release_gateway_wiring(&mut self, feature: &str) -> Result<()> {
        if self.service_capabilities.is_empty() {
            let path = self.registrations_path();
            if let Some(text) = self.read(&path)? {
                let keys = in_place::snippet_keys(GATEWAY_ENV_BLOCK)?;
                let ours: Vec<&str> = keys
                    .iter()
                    .map(String::as_str)
                    .filter(|key| {
                        in_place::entry_matches_snippet(
                            &text,
                            "environmentConfig",
                            key,
                            GATEWAY_ENV_BLOCK,
                        )
                    })
                    .collect();
                let removed = in_place::remove_config_entries(&text, "environmentConfig", &ours)?;
                // Kept while anything else (another registration's factory)
                // still reads it.
                let unread: Vec<&str> = ours
                    .into_iter()
                    .filter(|key| !in_place::binding_used(&removed, key))
                    .collect();
                if !unread.is_empty() {
                    self.remove_registration(None, &unread, &[])?;
                }
            }
            let env_local = self.service_path.join(".env.local");
            if let Some(text) = self.read(&env_local)? {
                let pairs = local_gateway_env();
                let pairs: Vec<(&str, &str)> =
                    pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
                let changed = in_place::remove_env_pairs(&text, &pairs);
                if changed != text {
                    if changed.trim().is_empty() {
                        self.delete(env_local);
                    } else {
                        self.write(env_local, changed);
                    }
                }
            }
        }
        let keys = self.compose_keys.clone();
        let service_has_none_left = self.service_capabilities.is_empty();
        let app_has_none_left = self.app_capabilities.is_empty();
        let events = find(feature).is_none_or(|c| c.receives_events);
        self.update_compose(|compose| {
            for (i, key) in keys.iter().enumerate() {
                release_gateway_mock(
                    compose,
                    key,
                    feature,
                    events && i + 1 == keys.len(),
                    service_has_none_left,
                    app_has_none_left,
                );
            }
            Ok(())
        })
    }

    /// Change docker-compose.yaml through its model, writing it only when
    /// something changed. Serializing drops comments, so the file's leading
    /// comment block is kept.
    fn update_compose(
        &mut self,
        change: impl FnOnce(&mut DockerCompose) -> Result<()>,
    ) -> Result<()> {
        let path = self.app_root.join("docker-compose.yaml");
        let Some(text) = self.read(&path)? else {
            return Ok(());
        };
        let mut compose: DockerCompose = serde_yml::from_str(&text)?;
        let before = serde_yml::to_string(&compose)?;
        change(&mut compose)?;
        let after = serde_yml::to_string(&compose)?;
        if after != before {
            let header: String = text
                .split_inclusive('\n')
                .take_while(|l| l.starts_with('#') || l.trim().is_empty())
                .collect();
            self.write(path, format!("{header}{after}"));
        }
        Ok(())
    }

    /// Append `KEY=value` lines to the service's `.env.local` for keys it
    /// does not set yet.
    pub(crate) fn ensure_env_local(&mut self, pairs: &[(&str, &str)]) -> Result<()> {
        let path = self.service_path.join(".env.local");
        let mut text = self.read(&path)?.unwrap_or_default();
        let mut changed = false;
        for (key, value) in pairs {
            let present = text
                .lines()
                .any(|line| in_place::env_key(line) == Some(*key));
            if !present {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!("{key}={value}\n"));
                changed = true;
            }
        }
        if changed {
            self.write(path, text);
        }
        Ok(())
    }

    // ------------------------------------------------------------- events

    /// Give the service a `/platform-events/:feature` route (once) and a
    /// handler file for `feature` (kept if it exists), and list it in the
    /// dispatch table. `handler_stub` is the new handler file's content.
    pub(crate) fn ensure_platform_events(
        &mut self,
        feature: &str,
        handler_stub: &str,
    ) -> Result<()> {
        let routes = self
            .service_path
            .join("api/routes/platformEvents.routes.ts");
        if self.read(&routes)?.is_none() {
            self.write(routes, render(PLATFORM_EVENTS_ROUTES, &self.app_name));
            self.write(
                self.service_path
                    .join("api/controllers/platformEvents.controller.ts"),
                render(PLATFORM_EVENTS_CONTROLLER, &self.app_name),
            );
            self.mount_platform_events_router()?;
        }
        let handler = self
            .service_path
            .join(format!("api/platformEvents/{feature}.ts"));
        if self.read(&handler)?.is_none() {
            self.write(handler, handler_stub.to_string());
        }
        self.write_platform_events_index()?;
        Ok(())
    }

    /// Drop `feature` from the dispatch table (its handler file is deleted),
    /// and the route itself when no event feature is left.
    pub(crate) fn remove_platform_events(&mut self, feature: &str) -> Result<()> {
        self.delete(
            self.service_path
                .join(format!("api/platformEvents/{feature}.ts")),
        );
        let remaining = self.event_features();
        if remaining.is_empty() {
            for file in [
                "api/routes/platformEvents.routes.ts",
                "api/controllers/platformEvents.controller.ts",
                "api/platformEvents/index.ts",
            ] {
                self.delete(self.service_path.join(file));
            }
            let server = self.service_path.join("server.ts");
            if let Some(text) = self.read(&server)? {
                // The two lines `mount_platform_events_router` wrote, matched
                // by content (they follow the file's indent, quotes and
                // semicolons).
                let kept: String = text
                    .split_inclusive('\n')
                    .filter(|line| !is_platform_events_mount(line))
                    .collect();
                if kept != text {
                    self.write(server, kept);
                }
            }
        } else {
            self.write_platform_events_index()?;
        }
        Ok(())
    }

    fn event_features(&self) -> Vec<String> {
        self.service_capabilities
            .iter()
            .filter(|c| find(c).is_some_and(|c| c.receives_events))
            .cloned()
            .collect()
    }

    fn write_platform_events_index(&mut self) -> Result<()> {
        let features = self.event_features();
        let mut imports = String::new();
        let mut entries = String::new();
        for feature in &features {
            let ident = camel(feature);
            imports.push_str(&format!(
                "import {{ handle as {ident} }} from './{feature}';\n"
            ));
            entries.push_str(&format!("  {feature}: {ident},\n"));
        }
        let content = format!(
            "/**\n * Platform event handlers by feature. Generated by `forklaunch infra add`;\n * edit the handler files, not this list.\n */\nimport type {{ PlatformEvent }} from '@forklaunch/core/http';\n{imports}\nexport const platformEventHandlers: Record<\n  string,\n  (event: PlatformEvent) => Promise<void>\n> = {{\n{entries}}};\n"
        );
        self.write(
            self.service_path.join("api/platformEvents/index.ts"),
            content,
        );
        Ok(())
    }

    fn mount_platform_events_router(&mut self) -> Result<()> {
        let server = self.service_path.join("server.ts");
        let Some(text) = self.read(&server)? else {
            bail!("{server:?} not found; mount platformEventsRouter by hand");
        };
        if text.contains("platformEventsRouter") {
            return Ok(());
        }
        // After the last import, and after the first `app.use(` line, in the
        // file's quotes and semicolons; every other line is left as it is.
        let q = in_place::quote_of(&text);
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let import_ends: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|l| l.starts_with("import ") || l.starts_with("} from "))
            .filter(|l| l.contains(" from "))
            .collect();
        let semi = if import_ends.is_empty() || import_ends.iter().any(|l| l.trim_end().ends_with(';')) {
            ";"
        } else {
            ""
        };
        let last_import = lines
            .iter()
            .rposition(|l| l.starts_with("import ") || l.starts_with("} from "));
        let import = format!("import {{ platformEventsRouter }} from {q}./api/routes/platformEvents.routes{q}{semi}\n");
        let mut out = String::new();
        if last_import.is_none() {
            out.push_str(&import);
        }
        let mut used = false;
        for (i, line) in lines.iter().enumerate() {
            out.push_str(line);
            let after = if Some(i) == last_import {
                Some(import.clone())
            } else if !used && line.trim_start().starts_with("app.use(") {
                used = true;
                let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                Some(format!("{indent}app.use(platformEventsRouter){semi}\n"))
            } else {
                None
            };
            if let Some(after) = after {
                if !line.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&after);
            }
        }
        if !used {
            bail!("no app.use( in {server:?}; mount platformEventsRouter by hand");
        }
        self.write(server, out);
        Ok(())
    }
}

/// A line `mount_platform_events_router` writes into server.ts.
fn is_platform_events_mount(line: &str) -> bool {
    let line = line.trim();
    let line = line.strip_suffix(';').unwrap_or(line).replace('"', "'");
    line == "app.use(platformEventsRouter)"
        || line == "import { platformEventsRouter } from './api/routes/platformEvents.routes'"
}

/// The `.env.local` values a local service reaches the gateway mock with.
fn local_gateway_env() -> Vec<(&'static str, String)> {
    vec![
        (
            "PLATFORM_GATEWAY_URL",
            format!("http://localhost:{GATEWAY_MOCK_HOST_PORT}"),
        ),
        ("INSTANCE_ID", LOCAL_INSTANCE_ID.to_string()),
        ("INSTANCE_HMAC_KEY", LOCAL_INSTANCE_HMAC_KEY.to_string()),
    ]
}

/// The names a one-line `import { a, b } from "x";` brings in.
fn named_imports(import_line: &str) -> Vec<String> {
    let Some(open) = import_line.find('{') else {
        return Vec::new();
    };
    let Some(close) = import_line[open..].find('}') else {
        return Vec::new();
    };
    import_line[open + 1..open + close]
        .split(',')
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

/// The name a one-line default `import X from "x";` brings in.
fn default_import(import_line: &str) -> Option<String> {
    regex::Regex::new(r"^\s*import\s+([A-Za-z_$][\w$]*)\s+from\b")
        .unwrap()
        .captures(import_line)
        .map(|c| c[1].to_string())
}

/// The value import from `source` (not `import type`), as (start, end, names).
pub(crate) fn find_import(text: &str, source: &str) -> Option<(usize, usize, Vec<String>)> {
    let pattern = format!(
        r#"import\s*\{{([^}}]*)\}}\s*from\s*["']{}["'];?"#,
        regex::escape(source)
    );
    let re = regex::Regex::new(&pattern).ok()?;
    let m = re.captures(text)?;
    let whole = m.get(0)?;
    let names = m[1]
        .split(',')
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    Some((whole.start(), whole.end(), names))
}

/// Add `names` to the file's import from `source`, or add that import, in
/// the file's style (see `in_place::add_named_imports`).
pub(crate) fn add_named_imports(text: &str, source: &str, names: &[&str]) -> String {
    in_place::add_named_imports(text, source, names)
}

/// Drop `names` from the file's import from `source`, and the import if empty.
#[cfg(test)]
pub(crate) fn remove_named_imports(text: &str, source: &str, names: &[&str]) -> String {
    in_place::remove_named_imports(text, source, names)
}

fn camel(feature: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in feature.chars() {
        if c == '-' || c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    format!("{out}Events")
}

fn render(template: &str, app_name: &str) -> String {
    template.replace("{{app_name}}", app_name)
}

/// The service and mock sides of the local gateway in docker-compose.
/// The mock's event destinations for a feature: `MOCK_EVENTS_URL_<FEATURE>`,
/// a comma-separated list, one entry per project that holds the feature
/// (the mock delivers each event to every entry).
fn events_var(feature: &str) -> String {
    format!("MOCK_EVENTS_URL_{}", feature.to_uppercase().replace('-', "_"))
}

fn events_url(compose: &DockerCompose, service_key: &str) -> Option<String> {
    let service = compose.services.get(service_key)?;
    let port = service
        .environment
        .as_ref()
        .and_then(|e| e.get("PORT").cloned())
        .unwrap_or_else(|| "8000".to_string());
    Some(format!("http://{service_key}:{port}"))
}

fn url_list(value: Option<&String>) -> Vec<String> {
    value
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The gateway mock service `infra add` creates, on `networks`, before any
/// project registers an event destination.
pub(crate) fn generated_gateway_mock(networks: Option<Vec<String>>) -> DockerService {
    DockerService {
        hostname: Some(GATEWAY_MOCK_SERVICE.to_string()),
        image: Some("node:22-alpine".to_string()),
        command: Some(crate::core::docker::Command::Multiple(vec![
            "npx".to_string(),
            "-y".to_string(),
            "-p".to_string(),
            format!(
                "@forklaunch/core@{}",
                crate::core::package_json::package_json_constants::CORE_VERSION
            ),
            "forklaunch-gateway-mock".to_string(),
        ])),
        ports: Some(vec![format!("{GATEWAY_MOCK_HOST_PORT}:8080")]),
        networks,
        environment: Some(
            [
                ("PORT".to_string(), "8080".to_string()),
                (
                    "MOCK_INSTANCE_HMAC_KEY".to_string(),
                    LOCAL_INSTANCE_HMAC_KEY.to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        ),
        ..Default::default()
    }
}

/// Wire one container to the local gateway mock. `receives_events`: this
/// container mounts `/platform-events` and is added to the feature's event
/// destinations (never replacing another project's).
pub(crate) fn ensure_gateway_mock(
    compose: &mut DockerCompose,
    service_key: &str,
    feature: &str,
    receives_events: bool,
) -> Result<()> {
    let service = compose
        .services
        .get(service_key)
        .with_context(|| format!("no '{service_key}' service in docker-compose"))?
        .clone();
    let port = service
        .environment
        .as_ref()
        .and_then(|e| e.get("PORT").cloned())
        .unwrap_or_else(|| "8000".to_string());

    let mock = compose
        .services
        .entry(GATEWAY_MOCK_SERVICE.to_string())
        .or_insert_with(|| generated_gateway_mock(service.networks.clone()));
    if receives_events {
        let url = format!("http://{service_key}:{port}");
        let env = mock.environment.get_or_insert_with(Default::default);
        let mut urls = url_list(env.get(&events_var(feature)));
        if !urls.contains(&url) {
            urls.push(url);
        }
        env.insert(events_var(feature), urls.join(","));
    }

    let target = compose.services.get_mut(service_key).unwrap();
    let env = target.environment.get_or_insert_with(Default::default);
    // A value the service already sets is the developer's: kept (and left
    // on remove), with a note when it is not the mock's.
    for (key, value) in compose_gateway_env() {
        match env.get(key) {
            None => {
                env.insert(key.to_string(), value);
            }
            Some(existing) if *existing != value => eprintln!(
                "note: docker-compose.yaml sets {key}={existing:?} for {service_key}; kept (the gateway mock expects {value:?})"
            ),
            Some(_) => {}
        }
    }
    target
        .depends_on
        .get_or_insert_with(Default::default)
        .entry(GATEWAY_MOCK_SERVICE.to_string())
        .or_insert(DependsOn {
            condition: DependencyCondition::ServiceStarted,
            ..Default::default()
        });
    Ok(())
}

/// Undo `ensure_gateway_mock` for one container: only its own event
/// destination goes; another project's stays.
pub(crate) fn release_gateway_mock(
    compose: &mut DockerCompose,
    service_key: &str,
    feature: &str,
    receives_events: bool,
    service_has_none_left: bool,
    app_has_none_left: bool,
) {
    let own_url = events_url(compose, service_key);
    if let (true, Some(mock)) = (
        receives_events,
        compose.services.get_mut(GATEWAY_MOCK_SERVICE),
    ) {
        if let Some(env) = mock.environment.as_mut() {
            let urls: Vec<String> = url_list(env.get(&events_var(feature)))
                .into_iter()
                .filter(|u| Some(u) != own_url.as_ref())
                .collect();
            if urls.is_empty() {
                env.shift_remove(&events_var(feature));
            } else {
                env.insert(events_var(feature), urls.join(","));
            }
        }
    }
    if service_has_none_left {
        if let Some(target) = compose.services.get_mut(service_key) {
            if let Some(env) = target.environment.as_mut() {
                for (key, value) in compose_gateway_env() {
                    if env.get(key) == Some(&value) {
                        env.shift_remove(key);
                    }
                }
            }
            if let Some(deps) = target.depends_on.as_mut() {
                deps.shift_remove(GATEWAY_MOCK_SERVICE);
                if deps.is_empty() {
                    target.depends_on = None;
                }
            }
        }
    }
    if app_has_none_left {
        compose.services.shift_remove(GATEWAY_MOCK_SERVICE);
    }
}

/// Every name a capability's add imports into registrations.ts. On remove,
/// one of them goes when the removed registration was its last reader.
pub(crate) const ADDED_IMPORTS: &[&str] = &[
    "getEnvVar",
    "type",
    "createEmailClient",
    "EmailClient",
    "createSmsClient",
    "SmsClient",
    "createWhatsAppClient",
    "createVoiceClient",
    "VoiceClient",
    "createStripeClient",
    "Stripe",
    "createModelGatewayClient",
    "ModelGatewayClient",
];

/// The gateway settings a service gets in docker-compose.
fn compose_gateway_env() -> [(&'static str, String); 3] {
    [
        (
            "PLATFORM_GATEWAY_URL",
            format!("http://{GATEWAY_MOCK_SERVICE}:8080"),
        ),
        ("INSTANCE_ID", LOCAL_INSTANCE_ID.to_string()),
        ("INSTANCE_HMAC_KEY", LOCAL_INSTANCE_HMAC_KEY.to_string()),
    ]
}

const GATEWAY_ENV_BLOCK: &str = "const configInjector = createConfigInjector(SchemaValidator(), {
    PLATFORM_GATEWAY_URL: {
        lifetime: Lifetime.Singleton,
        type: optional(string),
        value: getEnvVar('PLATFORM_GATEWAY_URL')
    },
    INSTANCE_ID: {
        lifetime: Lifetime.Singleton,
        type: optional(string),
        value: getEnvVar('INSTANCE_ID')
    },
    INSTANCE_HMAC_KEY: {
        lifetime: Lifetime.Singleton,
        type: optional(string),
        value: getEnvVar('INSTANCE_HMAC_KEY')
    }
});";

const PLATFORM_EVENTS_ROUTES: &str = "import { forklaunchRouter, schemaValidator } from '@{{app_name}}/core';
import { ci, tokens } from '../../bootstrapper';
import { receivePlatformEvent } from '../controllers/platformEvents.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

/** Events the ForkLaunch platform delivers (payments, email, sms, …). */
export const platformEventsRouter = forklaunchRouter(
  '/platform-events',
  schemaValidator,
  openTelemetryCollector
);

export const receivePlatformEventRoute = platformEventsRouter.post(
  '/:feature',
  receivePlatformEvent
);
";

const PLATFORM_EVENTS_CONTROLLER: &str = "import { handlers, schemaValidator, string } from '@{{app_name}}/core';
import {
  PlatformEventVerificationError,
  verifyPlatformEvent
} from '@forklaunch/core/http';
import { platformEventHandlers } from '../platformEvents';

/**
 * Receives an event the ForkLaunch platform delivered, verifies it was signed
 * with this instance's key, and hands it to the feature's handler
 * (api/platformEvents/<feature>.ts). Deliveries can repeat: handlers dedupe
 * on event.id. A non-2xx answer makes the platform retry.
 */
export const receivePlatformEvent = handlers.post(
  schemaValidator,
  '/:feature',
  {
    name: 'ReceivePlatformEvent',
    access: 'public',
    summary: 'Receives a signed event from the ForkLaunch platform',
    params: { feature: string },
    body: schemaValidator.unknown,
    responses: { 200: string, 401: string, 404: string }
  },
  async (req, res) => {
    const handler = platformEventHandlers[req.params.feature];
    if (!handler) {
      res.status(404).send(`No handler for ${req.params.feature} events`);
      return;
    }
    let event;
    try {
      event = verifyPlatformEvent({
        method: 'POST',
        path: `/platform-events/${req.params.feature}`,
        headers: req.headers as Record<string, string | string[] | undefined>,
        body: (req as { _rawBody?: Buffer })._rawBody ?? req.body
      });
    } catch (error) {
      if (error instanceof PlatformEventVerificationError) {
        res.status(401).send(error.message);
        return;
      }
      throw error;
    }
    await handler(event);
    res.status(200).send('ok');
  }
);
";

#[cfg(test)]
mod tests;
