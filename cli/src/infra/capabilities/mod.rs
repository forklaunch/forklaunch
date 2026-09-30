//! Platform-held capabilities a service reaches through the instance gateway:
//! payments, email, sms, whatsapp, voice.
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

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use oxc_allocator::Allocator;
use oxc_ast::ast::SourceType;
use oxc_codegen::{Codegen, CodegenOptions};

use crate::core::{
    ast::{
        deletions::{
            delete_from_registrations_ts::delete_from_registrations_ts_config_injector,
            delete_import_statement::delete_import_statement,
        },
        injections::inject_into_registrations_ts::inject_into_registrations_config_injector,
        parse_ast_program::parse_ast_program,
        replacements::replace_import_statment::replace_import_statment,
    },
    docker::{DependencyCondition, DependsOn, DockerCompose, DockerService},
};

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
    /// Capability-specific edits on add; the shared ones are applied around it.
    pub(crate) add: fn(&mut CapabilityEdit) -> Result<()>,
    /// Capability-specific edits on remove.
    pub(crate) remove: fn(&mut CapabilityEdit) -> Result<()>,
}

/// Every capability `infra add` knows. Each feature adds its entry.
pub(crate) static CAPABILITIES: &[&Capability] = &[];

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
    pub(crate) app_root: PathBuf,
    pub(crate) app_name: String,
    pub(crate) service_name: String,
    pub(crate) service_path: PathBuf,
    /// Capabilities of this service after the change.
    pub(crate) service_capabilities: Vec<String>,
    /// Capabilities of every project after the change.
    pub(crate) app_capabilities: Vec<String>,
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
            app_root: app_root.to_path_buf(),
            app_name: app_name.to_string(),
            service_name: service_name.to_string(),
            service_path: service_path.to_path_buf(),
            service_capabilities,
            app_capabilities,
            staged: BTreeMap::new(),
        }
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

    pub(crate) fn write(&mut self, path: PathBuf, content: String) {
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
            .map(|(p, c)| (p.clone(), c.is_some()))
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
                    }
                }
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------- code

    /// Add an import and config-injector entries to registrations.ts.
    ///
    /// `env_block` and `runtime_block` are complete
    /// `const configInjector = createConfigInjector(SchemaValidator(), { … });`
    /// snippets whose entries are merged into `environmentConfig` and
    /// `runtimeDependencies`. Entries already present are left alone.
    pub(crate) fn inject_registration(
        &mut self,
        import_line: &str,
        import_source: &str,
        env_block: Option<&str>,
        runtime_block: Option<&str>,
    ) -> Result<()> {
        let path = self.registrations_path();
        let text = self
            .read(&path)?
            .with_context(|| format!("{path:?} not found"))?;
        let allocator = Allocator::default();
        let text: &str = Box::leak(text.into_boxed_str());
        let mut program = parse_ast_program(&allocator, text, SourceType::ts());
        if !text.contains(import_line) {
            let import: &str = Box::leak(import_line.to_string().into_boxed_str());
            let mut import_program = parse_ast_program(&allocator, import, SourceType::ts());
            let _ = replace_import_statment(&mut program, &mut import_program, import_source);
        }
        for (block, declaration) in [
            (env_block, "environmentConfig"),
            (runtime_block, "runtimeDependencies"),
        ] {
            let Some(block) = block else { continue };
            let block: &str = Box::leak(block.to_string().into_boxed_str());
            let mut block_program = parse_ast_program(&allocator, block, SourceType::ts());
            inject_into_registrations_config_injector(
                &allocator,
                &mut program,
                &mut block_program,
                declaration,
            )?;
        }
        let code = Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&program)
            .code;
        self.write(path, code);
        Ok(())
    }

    /// Remove config-injector entries (and optionally an import).
    pub(crate) fn remove_registration(
        &mut self,
        import_source: Option<&str>,
        env_keys: &[&str],
        runtime_keys: &[&str],
    ) -> Result<()> {
        let path = self.registrations_path();
        let Some(text) = self.read(&path)? else {
            return Ok(());
        };
        let allocator = Allocator::default();
        let text: &str = Box::leak(text.into_boxed_str());
        let mut program = parse_ast_program(&allocator, text, SourceType::ts());
        for key in env_keys {
            let _ = delete_from_registrations_ts_config_injector(
                &allocator,
                &mut program,
                key,
                "environmentConfig",
            );
        }
        for key in runtime_keys {
            let _ = delete_from_registrations_ts_config_injector(
                &allocator,
                &mut program,
                key,
                "runtimeDependencies",
            );
        }
        if let Some(source) = import_source {
            let _ = delete_import_statement(&allocator, &mut program, source);
        }
        let code = Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&program)
            .code;
        self.write(path, code);
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
        self.ensure_env_local(&[
            ("PLATFORM_GATEWAY_URL", &format!("http://localhost:{GATEWAY_MOCK_HOST_PORT}")),
            ("INSTANCE_ID", LOCAL_INSTANCE_ID),
            ("INSTANCE_HMAC_KEY", LOCAL_INSTANCE_HMAC_KEY),
        ])?;
        let compose_path = self.app_root.join("docker-compose.yaml");
        let Some(text) = self.read(&compose_path)? else {
            return Ok(());
        };
        let mut compose: DockerCompose = serde_yml::from_str(&text)?;
        ensure_gateway_mock(&mut compose, &self.service_name, feature)?;
        self.write(compose_path, serde_yml::to_string(&compose)?);
        Ok(())
    }

    /// Undo `ensure_gateway_wiring` for `feature`, dropping the service's
    /// gateway settings when it has no capability left, and the mock when
    /// the app has none.
    pub(crate) fn release_gateway_wiring(&mut self, feature: &str) -> Result<()> {
        if self.service_capabilities.is_empty() {
            self.remove_registration(
                None,
                &["PLATFORM_GATEWAY_URL", "INSTANCE_ID", "INSTANCE_HMAC_KEY"],
                &[],
            )?;
        }
        let compose_path = self.app_root.join("docker-compose.yaml");
        let Some(text) = self.read(&compose_path)? else {
            return Ok(());
        };
        let mut compose: DockerCompose = serde_yml::from_str(&text)?;
        release_gateway_mock(
            &mut compose,
            &self.service_name,
            feature,
            self.service_capabilities.is_empty(),
            self.app_capabilities.is_empty(),
        );
        self.write(compose_path, serde_yml::to_string(&compose)?);
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
                .any(|l| l.trim_start().starts_with(&format!("{key}=")));
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
                let text = text
                    .replace(PLATFORM_EVENTS_IMPORT, "")
                    .replace(PLATFORM_EVENTS_USE, "");
                self.write(server, text);
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
        // After the last import, and after the first `app.use(` line.
        let mut out = String::new();
        let lines: Vec<&str> = text.lines().collect();
        let last_import = lines
            .iter()
            .rposition(|l| l.starts_with("import ") || l.starts_with("} from "))
            .unwrap_or(0);
        let mut used = false;
        for (i, line) in lines.iter().enumerate() {
            out.push_str(line);
            out.push('\n');
            if i == last_import {
                out.push_str(PLATFORM_EVENTS_IMPORT);
            }
            if !used && line.trim_start().starts_with("app.use(") {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                out.push_str(&format!("{indent}{}", PLATFORM_EVENTS_USE.trim_start()));
                used = true;
            }
        }
        if !used {
            bail!("no app.use( in {server:?}; mount platformEventsRouter by hand");
        }
        self.write(server, out);
        Ok(())
    }
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
pub(crate) fn ensure_gateway_mock(
    compose: &mut DockerCompose,
    service_key: &str,
    feature: &str,
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
        .or_insert_with(|| DockerService {
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
            networks: service.networks.clone(),
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
        });
    mock.environment.get_or_insert_with(Default::default).insert(
        format!("MOCK_EVENTS_URL_{}", feature.to_uppercase().replace('-', "_")),
        format!("http://{service_key}:{port}"),
    );

    let target = compose.services.get_mut(service_key).unwrap();
    let env = target.environment.get_or_insert_with(Default::default);
    env.insert(
        "PLATFORM_GATEWAY_URL".to_string(),
        format!("http://{GATEWAY_MOCK_SERVICE}:8080"),
    );
    env.insert("INSTANCE_ID".to_string(), LOCAL_INSTANCE_ID.to_string());
    env.insert(
        "INSTANCE_HMAC_KEY".to_string(),
        LOCAL_INSTANCE_HMAC_KEY.to_string(),
    );
    target
        .depends_on
        .get_or_insert_with(Default::default)
        .entry(GATEWAY_MOCK_SERVICE.to_string())
        .or_insert(DependsOn {
            condition: DependencyCondition::ServiceStarted,
        });
    Ok(())
}

pub(crate) fn release_gateway_mock(
    compose: &mut DockerCompose,
    service_key: &str,
    feature: &str,
    service_has_none_left: bool,
    app_has_none_left: bool,
) {
    if let Some(mock) = compose.services.get_mut(GATEWAY_MOCK_SERVICE) {
        if let Some(env) = mock.environment.as_mut() {
            env.shift_remove(&format!(
                "MOCK_EVENTS_URL_{}",
                feature.to_uppercase().replace('-', "_")
            ));
        }
    }
    if service_has_none_left {
        if let Some(target) = compose.services.get_mut(service_key) {
            if let Some(env) = target.environment.as_mut() {
                for key in ["PLATFORM_GATEWAY_URL", "INSTANCE_ID", "INSTANCE_HMAC_KEY"] {
                    env.shift_remove(key);
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

const PLATFORM_EVENTS_IMPORT: &str =
    "import { platformEventsRouter } from './api/routes/platformEvents.routes';\n";
const PLATFORM_EVENTS_USE: &str = "  app.use(platformEventsRouter);\n";

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
mod tests {
    use super::*;

    fn compose() -> DockerCompose {
        serde_yml::from_str(
            "volumes: {}\nnetworks:\n  demo-network:\n    name: demo-network\nservices:\n  billing:\n    image: node:22\n    networks: [demo-network]\n    environment:\n      PORT: '8001'\n",
        )
        .unwrap()
    }

    #[test]
    fn gateway_mock_is_added_once_and_removed_with_the_last_capability() {
        let mut c = compose();
        ensure_gateway_mock(&mut c, "billing", "payments").unwrap();
        ensure_gateway_mock(&mut c, "billing", "email").unwrap();
        let mock = &c.services[GATEWAY_MOCK_SERVICE];
        let env = mock.environment.as_ref().unwrap();
        assert_eq!(env["MOCK_EVENTS_URL_PAYMENTS"], "http://billing:8001");
        assert_eq!(env["MOCK_EVENTS_URL_EMAIL"], "http://billing:8001");
        assert_eq!(env["MOCK_INSTANCE_HMAC_KEY"], LOCAL_INSTANCE_HMAC_KEY);
        let billing = c.services["billing"].environment.as_ref().unwrap();
        assert_eq!(billing["PLATFORM_GATEWAY_URL"], "http://gateway-mock:8080");
        assert_eq!(billing["INSTANCE_HMAC_KEY"], LOCAL_INSTANCE_HMAC_KEY);

        release_gateway_mock(&mut c, "billing", "email", false, false);
        assert!(c.services.contains_key(GATEWAY_MOCK_SERVICE));
        assert!(c.services["billing"].environment.as_ref().unwrap().contains_key("INSTANCE_ID"));
        release_gateway_mock(&mut c, "billing", "payments", true, true);
        assert!(!c.services.contains_key(GATEWAY_MOCK_SERVICE));
        assert!(!c.services["billing"].environment.as_ref().unwrap().contains_key("INSTANCE_ID"));
        assert!(c.services["billing"].depends_on.is_none());
    }

    #[test]
    fn unknown_capabilities_keep_their_id_as_resource_type() {
        assert_eq!(resource_type("carrier-pigeon"), "carrier-pigeon");
        assert_eq!(camel("whats-app"), "whatsAppEvents");
    }
}
