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
    ensure_gateway_mock(&mut c, "billing", "payments", true).unwrap();
    ensure_gateway_mock(&mut c, "billing", "email", true).unwrap();
    let mock = &c.services[GATEWAY_MOCK_SERVICE];
    let env = mock.environment.as_ref().unwrap();
    assert_eq!(env["MOCK_EVENTS_URL_PAYMENTS"], "http://billing:8001");
    assert_eq!(env["MOCK_EVENTS_URL_EMAIL"], "http://billing:8001");
    assert_eq!(env["MOCK_INSTANCE_HMAC_KEY"], LOCAL_INSTANCE_HMAC_KEY);
    let billing = c.services["billing"].environment.as_ref().unwrap();
    assert_eq!(billing["PLATFORM_GATEWAY_URL"], "http://gateway-mock:8080");
    assert_eq!(billing["INSTANCE_HMAC_KEY"], LOCAL_INSTANCE_HMAC_KEY);

    release_gateway_mock(&mut c, "billing", "email", true, false, false);
    assert!(c.services.contains_key(GATEWAY_MOCK_SERVICE));
    assert!(c.services["billing"].environment.as_ref().unwrap().contains_key("INSTANCE_ID"));
    release_gateway_mock(&mut c, "billing", "payments", true, true, true);
    assert!(!c.services.contains_key(GATEWAY_MOCK_SERVICE));
    assert!(!c.services["billing"].environment.as_ref().unwrap().contains_key("INSTANCE_ID"));
    assert!(c.services["billing"].depends_on.is_none());
}

#[test]
fn unknown_capabilities_keep_their_id_as_resource_type() {
    assert_eq!(resource_type("carrier-pigeon"), "carrier-pigeon");
    assert_eq!(camel("whats-app"), "whatsAppEvents");
}

// ------------------------------------------------ in-place round trips

/// A customized service in the style biome writes: header doc comments,
/// inline comments, two-space indentation, single quotes, no trailing
/// commas, custom registrations.
const CUSTOM_REGISTRATIONS: &str = r#"/**
 * Clinic service registrations.
 * Feature-specific DI is appended below the runtime base.
 */

import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  FieldEncryptor,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import {
  Lifetime,
  createConfigInjector,
  getEnvVar
} from '@forklaunch/core/services';
import { number, optional, schemaValidator, string } from '@clinic/core';
import { metrics } from '@clinic/monitoring';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import { FactsService } from './domain/services/facts.service';
import mikroOrmOptionsConfig from './mikro-orm.config';

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
lifetime: Lifetime.Singleton,
type: { name: string, version: string },
value: { name: 'clinic', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
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
  new OpenTelemetryCollector('clinic', OTEL_LEVEL || 'info', metrics)
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

const CUSTOM_SERVER: &str = r#"/**
 * Clinic service entrypoint.
 */

import {
  forklaunchExpress,
  schemaValidator
} from '@clinic/core';
import { factsRouter } from './api/routes/facts.routes';
import { ci, tokens } from './bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const app = forklaunchExpress(schemaValidator, openTelemetryCollector);

// Domain routers.
app.use(factsRouter);

app.listen(ci.resolve(tokens.PORT), () => {
  openTelemetryCollector.info('up');
});
"#;

const CUSTOM_PACKAGE_JSON: &str = r#"{
  "name": "@clinic/clinic",
  "scripts": {
"zeta": "run zeta",
"alpha": "run alpha"
  },
  "dependencies": {
"@clinic/core": "workspace:*",
"@forklaunch/core": "~3.0.1",
"@mikro-orm/postgresql": "^7.2.0",
"zod": "^4.4.3"
  }
}
"#;

/// docker-compose.yaml is edited through its model (keeping the header),
/// so the fixture is in the form the CLI writes it.
fn custom_compose() -> String {
    let body = "volumes: {}\nnetworks:\n  clinic-network:\n    name: clinic-network\nservices:\n  clinic:\n    image: node:22\n    environment:\n      PORT: '8002'\n      INSTANCE_ID: ''\n    networks:\n    - clinic-network\n";
    let compose: DockerCompose = serde_yml::from_str(body).unwrap();
    format!(
        "# Generated by ForkLaunch\n# File: docker-compose.yaml\n\n{}",
        serde_yml::to_string(&compose).unwrap()
    )
}

const CUSTOM_ENV_LOCAL: &str = "PORT=8002\n# local only\nOTEL_LEVEL=debug\n";

fn custom_service(registrations: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let service = dir.path().join("src/modules/clinic");
    fs::create_dir_all(&service).unwrap();
    fs::write(service.join("registrations.ts"), registrations).unwrap();
    fs::write(service.join("server.ts"), CUSTOM_SERVER).unwrap();
    fs::write(service.join("package.json"), CUSTOM_PACKAGE_JSON).unwrap();
    fs::write(service.join(".env.local"), CUSTOM_ENV_LOCAL).unwrap();
    fs::write(dir.path().join("docker-compose.yaml"), custom_compose()).unwrap();
    (dir, service)
}

fn run(root: &Path, service: &Path, capability: &Capability, caps: &[&str], adding: bool) {
    let caps: Vec<String> = caps.iter().map(|c| c.to_string()).collect();
    let mut edit = CapabilityEdit::new(root, "clinic", "clinic", service, caps.clone(), caps);
    apply(&mut edit, capability, adding).unwrap();
    edit.commit().unwrap();
}

/// Every line of `before` is still in `after`, in the same order; a
/// one-line named import may have gained names (it keeps its own).
fn assert_lines_kept(before: &str, after: &str, what: &str) {
    let one_line_import =
        regex::Regex::new(r"^import \{ ([^}]*) \} from ('[^']+');$").unwrap();
    let mut rest = after;
    for line in before.lines() {
        if let Some(c) = one_line_import.captures(line) {
            let merged = regex::Regex::new(&format!(
                r"(?s)import \{{([^}}]*)\}} from {};",
                regex::escape(&c[2])
            ))
            .unwrap();
            let names = merged
                .captures(after)
                .unwrap_or_else(|| panic!("{what}: lost {line:?}\n{after}"))[1]
                .to_string();
            for name in c[1].split(", ") {
                assert!(
                    names.split(',').any(|n| n.trim() == name),
                    "{what}: {name} dropped from {line:?}\n{after}"
                );
            }
            continue;
        }
        let at = rest
            .find(line)
            .unwrap_or_else(|| panic!("{what}: lost or reordered line {line:?}\n{after}"));
        rest = &rest[at + line.len()..];
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, String> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.insert(path.clone(), fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

#[test]
fn every_capability_adds_in_place_and_removes_byte_for_byte() {
    for capability in CAPABILITIES {
        let (dir, service) = custom_service(CUSTOM_REGISTRATIONS);
        let before = snapshot(dir.path());
        run(dir.path(), &service, capability, &[capability.id], true);

        let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
        let id = capability.id;
        assert_lines_kept(CUSTOM_REGISTRATIONS, &registrations, id);
        // What the capability-wiring check looks for.
        assert!(
            registrations.contains(&format!("  {}: {{\n    lifetime:", capability.registration_key)),
            "{id}: registration not in the file's indent\n{registrations}"
        );
        assert!(registrations.contains("  PLATFORM_GATEWAY_URL: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('PLATFORM_GATEWAY_URL')\n  },"), "{id}\n{registrations}");
        assert!(!registrations.contains('"'), "{id}: switched quote style\n{registrations}");
        assert!(!registrations.contains('\t'), "{id}: switched indentation");
        assert!(
            !regex::Regex::new(r",\s*\n\s*[}\])]").unwrap().is_match(&registrations),
            "{id}: added a trailing comma\n{registrations}"
        );
        let server = fs::read_to_string(service.join("server.ts")).unwrap();
        if capability.receives_events {
            assert_lines_kept(CUSTOM_SERVER, &server, id);
            assert!(server.contains("import { ci, tokens } from './bootstrapper';\nimport { platformEventsRouter } from './api/routes/platformEvents.routes';\n"), "{id}\n{server}");
            assert!(server.contains("app.use(factsRouter);\napp.use(platformEventsRouter);\n"), "{id}\n{server}");
            assert_eq!(server.lines().count(), CUSTOM_SERVER.lines().count() + 2);
        } else {
            // No events: no route to mount.
            assert_eq!(server, CUSTOM_SERVER, "{id}");
            assert!(!service.join("api/platformEvents").exists(), "{id}");
        }
        let package_json = fs::read_to_string(service.join("package.json")).unwrap();
        assert_lines_kept(CUSTOM_PACKAGE_JSON, &package_json, id);
        assert!(package_json.lines().count() <= CUSTOM_PACKAGE_JSON.lines().count() + 1);
        let env_local = fs::read_to_string(service.join(".env.local")).unwrap();
        assert!(env_local.starts_with(CUSTOM_ENV_LOCAL));
        let compose = fs::read_to_string(dir.path().join("docker-compose.yaml")).unwrap();
        assert!(compose.starts_with("# Generated by ForkLaunch\n# File: docker-compose.yaml\n\n"));
        // The developer's own value is kept.
        assert!(compose.contains("INSTANCE_ID: ''"), "{compose}");

        run(dir.path(), &service, capability, &[], false);
        let after = snapshot(dir.path());
        for (path, content) in &before {
            assert_eq!(after.get(path), Some(content), "{id}: {path:?} changed");
        }
        let extra: Vec<&PathBuf> = after.keys().filter(|p| !before.contains_key(*p)).collect();
        assert!(extra.is_empty(), "{id}: left behind {extra:?}");
    }
}

#[test]
fn the_services_own_gateway_settings_are_left_alone() {
    // As in a service that reads the gateway itself: entries of its own
    // shape, read by another registration.
    let registrations = CUSTOM_REGISTRATIONS
        .replace(
            "  // Epic SMART/PKCE OAuth.",
            "  PLATFORM_GATEWAY_URL: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('PLATFORM_GATEWAY_URL') ?? undefined\n  },\n  INSTANCE_ID: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('INSTANCE_ID') ?? undefined\n  },\n  INSTANCE_HMAC_KEY: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('INSTANCE_HMAC_KEY') ?? undefined\n  },\n  // Epic SMART/PKCE OAuth.",
        );
    for capability in CAPABILITIES {
        let (dir, service) = custom_service(&registrations);
        let before = snapshot(dir.path());
        run(dir.path(), &service, capability, &[capability.id], true);
        let added = fs::read_to_string(service.join("registrations.ts")).unwrap();
        assert_eq!(added.matches("PLATFORM_GATEWAY_URL: {").count(), 1, "{}", capability.id);
        run(dir.path(), &service, capability, &[], false);
        let after = snapshot(dir.path());
        for (path, content) in &before {
            assert_eq!(after.get(path), Some(content), "{}: {path:?} changed", capability.id);
        }
    }
}

#[test]
fn capabilities_stack_and_come_off_in_any_order() {
    let (dir, service) = custom_service(CUSTOM_REGISTRATIONS);
    let before = snapshot(dir.path());
    let ids: Vec<&str> = CAPABILITIES.iter().map(|c| c.id).collect();
    for (i, capability) in CAPABILITIES.iter().enumerate() {
        run(dir.path(), &service, capability, &ids[..=i], true);
    }
    let registrations = fs::read_to_string(service.join("registrations.ts")).unwrap();
    assert_lines_kept(CUSTOM_REGISTRATIONS, &registrations, "all");
    // Removed in a different order than added.
    let mut left: Vec<&str> = ids.clone();
    for id in ["voice", "models", "email", "payments", "sms", "whatsapp"] {
        left.retain(|c| *c != id);
        run(dir.path(), &service, find(id).unwrap(), &left, false);
    }
    assert!(left.is_empty());
    let after = snapshot(dir.path());
    for (path, content) in &before {
        assert_eq!(after.get(path), Some(content), "{path:?} changed");
    }
}

#[test]
fn dryrun_staging_writes_nothing() {
    let (dir, service) = custom_service(CUSTOM_REGISTRATIONS);
    let before = snapshot(dir.path());
    let caps = vec!["email".to_string()];
    let mut edit = CapabilityEdit::new(dir.path(), "clinic", "clinic", &service, caps.clone(), caps);
    apply(&mut edit, &email::EMAIL, true).unwrap();
    let changed: Vec<PathBuf> = edit.changed_paths().into_iter().map(|(p, _)| p).collect();
    assert!(changed.contains(&service.join("registrations.ts")));
    assert!(changed.contains(&service.join("server.ts")));
    // package.json is untouched by email, so it is not staged either.
    assert!(!changed.contains(&service.join("package.json")));
    drop(edit);
    assert_eq!(snapshot(dir.path()), before);
}
