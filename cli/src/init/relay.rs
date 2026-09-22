//! Managed-apps OAuth relay session-ingest module.
//!
//! Unlike every other `Module`, relay does not scaffold a new service. It
//! injects the instance-side `/relay/session-ingest` endpoint (and its
//! browser-facing `/relay/handoff` redirect) into the app's EXISTING iam
//! service - the same shape Health Vault hand-built for managed-apps
//! readiness, generalized so the only app-specific decision is one hook.
//!
//! The generic ~80% it writes: the HMAC-verified ingest controller + route, the
//! nonce single-use replay guard (a unique-column handoff entity + migration),
//! the one-time handoff ticket + better-auth session-cookie minting, and the
//! root-relative redirect sanitizer. The app-specific ~20% is left as a single
//! clearly-marked hook (`establishSessionFromRelayTokens`) with a TODO.
//!
//! It is invoked through the module surface like any other module
//! (`forklaunch init module -m relay -p <app>`) but is special-cased in
//! `ModuleCommand::handler`, mirroring how the storefront subcommand extends an
//! existing app rather than generating a fresh project.

use std::{fs::read_to_string, io::Write, path::Path};

use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use termcolor::{Color, StandardStream, WriteColor};

use crate::{
    constants::{Database, Module, error_failed_to_read_file},
    core::{
        database::{get_database_port, get_db_driver, is_in_memory_database},
        format::format_code,
        manifest::{
            ManifestData, ProjectEntry, application::ApplicationManifestData,
            service::ServiceManifestData,
        },
        rendered_template::{RenderedTemplate, write_rendered_templates},
        template::{PathIO, generate_with_template},
    },
};

/// The service the relay endpoint is injected into. Managed apps put
/// better-auth sessions in `iam`, which is where the reference implementation
/// lives; keep this in one place so a future flag can target a different
/// auth/session service.
const AUTH_SERVICE_NAME: &str = "iam";

/// The better-auth iam variant tag, as written by `init module -m iam-better-auth`.
const BETTER_AUTH_VARIANT: &str = "iam-better-auth";

/// Adds the relay session-ingest endpoint to the app's iam service. Returns an
/// error (touching nothing) when there is no iam service to target or when the
/// endpoint is already installed.
pub(crate) fn add_relay_module(
    manifest_data: &ApplicationManifestData,
    base_path: &Path,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    let Some(iam_project) = manifest_data
        .projects
        .iter()
        .find(|project| project.name == AUTH_SERVICE_NAME)
    else {
        bail!(
            "No '{AUTH_SERVICE_NAME}' service found in this application. The relay module injects \
             its endpoint into an existing auth/session service - add one first with \
             `forklaunch init module -m iam-better-auth -p <app>`."
        );
    };

    let variant = iam_project.variant.clone().unwrap_or_default();
    let is_better_auth = variant == BETTER_AUTH_VARIANT;

    let iam_dir = base_path.join(AUTH_SERVICE_NAME);
    if !iam_dir.exists() {
        bail!(
            "Expected the iam service at {} but that directory does not exist.",
            iam_dir.display()
        );
    }

    // Idempotency: the endpoint lands as a fixed set of files; if the controller
    // is already there, a second run must not half-rewrite the wiring.
    let controller_path = iam_dir
        .join("api")
        .join("controllers")
        .join("relay.controller.ts");
    if controller_path.exists() {
        bail!(
            "The relay session-ingest endpoint is already installed in {} (found \
             api/controllers/relay.controller.ts).",
            iam_dir.display()
        );
    }

    let service_data = build_iam_service_manifest_data(manifest_data, iam_project);

    let template_dir = PathIO {
        input_path: Path::new("project")
            .join("relay")
            .to_string_lossy()
            .to_string(),
        output_path: iam_dir.to_string_lossy().to_string(),
        module_id: Some(Module::Relay),
    };

    let mut rendered_templates = generate_with_template(
        None,
        &template_dir,
        &ManifestData::Service(&service_data),
        &vec![],
        &vec![],
        &vec![],
        dryrun,
    )?;

    // Wire the generated files into the existing iam service. Each helper is a
    // no-op (returns None) when its anchor is already patched, so the whole
    // command is safe to re-run after a partial failure.
    if let Some(template) = inject_relay_into_server_ts(&iam_dir)? {
        rendered_templates.push(template);
    }
    if let Some(template) = inject_relay_into_registrations_ts(&iam_dir)? {
        rendered_templates.push(template);
    }
    if let Some(template) = inject_relay_into_entities_index(&iam_dir)? {
        rendered_templates.push(template);
    }
    // Barrel first, then the sdk that imports through it — the order does not
    // matter to the writer, but it is the order a reader needs.
    if let Some(template) = inject_claim_mint_into_controllers_index(&iam_dir)? {
        rendered_templates.push(template);
    }
    if let Some(template) = inject_claim_mint_into_sdk_ts(&iam_dir)? {
        rendered_templates.push(template);
    }

    write_rendered_templates(&rendered_templates, dryrun, stdout)?;

    if !dryrun {
        format_code(base_path, &service_data.runtime.parse()?);
        log_ok!(
            stdout,
            "managed-apps lifecycle endpoints (relay session-ingest, claim mint) injected into the {AUTH_SERVICE_NAME} service"
        );
        print_next_steps(stdout, is_better_auth)?;
    }

    Ok(())
}

/// Builds the render context for the iam service. Relay renders into iam, so it
/// needs iam's database (for the `sqlBaseProperties` vs `nosqlBaseProperties`
/// rewrite) and the app-level identity (for the `@forklaunch/blueprint-` ->
/// `@<app>/` rewrite). Everything else is a sensible default; the relay
/// templates only read `app_name` and `database`.
fn build_iam_service_manifest_data(
    manifest_data: &ApplicationManifestData,
    iam_project: &ProjectEntry,
) -> ServiceManifestData {
    let database_str = iam_project
        .resources
        .as_ref()
        .and_then(|resources| resources.database.clone())
        .unwrap_or_else(|| Database::PostgreSQL.to_string());
    let database: Database = database_str.parse().unwrap_or(Database::PostgreSQL);

    ServiceManifestData {
        id: manifest_data.id.clone(),
        cli_version: manifest_data.cli_version.clone(),
        app_name: manifest_data.app_name.clone(),
        modules_path: manifest_data.modules_path.clone(),
        docker_compose_path: manifest_data.docker_compose_path.clone(),
        dockerfile: manifest_data.dockerfile.clone(),
        git_repository: manifest_data.git_repository.clone(),
        camel_case_app_name: manifest_data.camel_case_app_name.clone(),
        pascal_case_app_name: manifest_data.pascal_case_app_name.clone(),
        kebab_case_app_name: manifest_data.kebab_case_app_name.clone(),
        title_case_app_name: manifest_data.title_case_app_name.clone(),
        service_name: AUTH_SERVICE_NAME.to_string(),
        service_path: AUTH_SERVICE_NAME.to_string(),
        camel_case_name: AUTH_SERVICE_NAME.to_case(Case::Camel),
        snake_case_name: AUTH_SERVICE_NAME.to_case(Case::Snake),
        pascal_case_name: AUTH_SERVICE_NAME.to_case(Case::Pascal),
        kebab_case_name: AUTH_SERVICE_NAME.to_case(Case::Kebab),
        title_case_name: AUTH_SERVICE_NAME.to_case(Case::Title),
        formatter: manifest_data.formatter.clone(),
        linter: manifest_data.linter.clone(),
        validator: manifest_data.validator.clone(),
        http_framework: manifest_data.http_framework.clone(),
        runtime: manifest_data.runtime.clone(),
        test_framework: manifest_data.test_framework.clone(),
        projects: manifest_data.projects.clone(),
        project_peer_topology: manifest_data.project_peer_topology.clone(),
        author: manifest_data.author.clone(),
        app_description: manifest_data.app_description.clone(),
        license: manifest_data.license.clone(),
        description: "managed-apps OAuth relay session-ingest".to_string(),

        is_eslint: manifest_data.is_eslint,
        is_biome: manifest_data.is_biome,
        is_oxlint: manifest_data.is_oxlint,
        is_prettier: manifest_data.is_prettier,
        is_express: manifest_data.is_express,
        is_hyper_express: manifest_data.is_hyper_express,
        is_zod: manifest_data.is_zod,
        is_typebox: manifest_data.is_typebox,
        is_bun: manifest_data.is_bun,
        is_node: manifest_data.is_node,
        is_vitest: manifest_data.is_vitest,
        is_jest: manifest_data.is_jest,

        is_postgres: database == Database::PostgreSQL,
        is_sqlite: database == Database::SQLite,
        is_mysql: database == Database::MySQL,
        is_mariadb: database == Database::MariaDB,
        is_better_sqlite: database == Database::BetterSQLite,
        is_libsql: database == Database::LibSQL,
        is_mssql: database == Database::MsSQL,
        is_mongo: database == Database::MongoDB,
        is_in_memory_database: is_in_memory_database(&database),

        database: database.to_string(),
        database_port: get_database_port(&database),
        db_driver: get_db_driver(&database),

        is_iam: true,
        is_billing: false,
        is_cache_enabled: false,
        is_s3_enabled: false,
        is_database_enabled: true,
        platform_application_id: manifest_data.platform_application_id.clone(),
        platform_organization_id: manifest_data.platform_organization_id.clone(),
        compliance: manifest_data.compliance.clone(),

        is_better_auth: iam_project.variant.as_deref() == Some(BETTER_AUTH_VARIANT),
        is_stripe: false,
        is_messaging: false,
        is_twilio: false,
        is_cac: false,
        is_ecommerce: false,
        ships_worker: false,

        is_iam_configured: true,
        is_billing_configured: manifest_data
            .projects
            .iter()
            .any(|project_entry| project_entry.name == "billing"),

        is_request_cache_needed: true,
        is_type_needed: true,

        with_mappers: false,

        iam_secret: None,

        generated_better_auth_secret: String::new(),
        generated_hmac_secret: String::new(),
        generated_encryption_key: String::new(),
        otel_token: "OtelCollector".to_string(),
    }
}

/// Adds the relay router mount and the browser-facing `/relay/handoff` redirect
/// to the iam `server.ts`. The handoff GET is a raw route (it must Set-Cookie +
/// 302, which a typed handler cannot), mirroring the existing `/api/auth/*` raw
/// routes.
fn inject_relay_into_server_ts(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let server_path = iam_dir.join("server.ts");
    let content = read_to_string(&server_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&server_path)))?;

    if content.contains("relayRouter") {
        return Ok(None);
    }

    // 1. Imports, appended after the last local import so ordering is stable.
    let import_anchor = "import { iamSdkClient } from './sdk';";
    if !content.contains(import_anchor) {
        bail!(
            "Could not find the expected import anchor in {}; refusing to guess where to wire the \
             relay in. Wire it by hand following the module docs.",
            server_path.display()
        );
    }
    let import_block = format!(
        "{import_anchor}\nimport {{ relayRouter }} from './api/routes/relay.routes';\nimport {{ serializeSessionCookie }} from './domain/services/relaySession.service';"
    );
    let mut updated = content.replace(import_anchor, &import_block);

    // 2. The browser-facing handoff redirect, placed just before the routes are
    // mounted so it wins over any catch-all.
    let mount_anchor = "//! mounts the routes to the app";
    if !updated.contains(mount_anchor) {
        bail!(
            "Could not find the route-mount anchor in {}; wire the relay by hand.",
            server_path.display()
        );
    }
    let handoff_route = r#"//! Managed-apps relay handoff: redeems a one-time ticket minted by
//! /relay/session-ingest, sets the better-auth session cookie, and 302s the
//! browser to a sanitized root-relative path. A raw route because it must
//! Set-Cookie + redirect.
app.internal.get('/relay/handoff', async (req, res) => {
  const ticket = String(req.query.ticket || '');
  if (!ticket) {
    res.redirect('/');
    return;
  }
  try {
    const relaySessionService = ci.scopedResolver(tokens.RelaySessionService)();
    const result = await relaySessionService.redeemTicket(ticket);
    if (!result) {
      res.redirect('/');
      return;
    }
    if (result.cookie) {
      res.setHeader('Set-Cookie', serializeSessionCookie(result.cookie));
    }
    res.redirect(result.redirectTo);
  } catch (err) {
    openTelemetryCollector.error(
      'Relay handoff failed',
      err instanceof Error ? err : new Error(String(err))
    );
    res.redirect('/');
  }
});

"#;
    updated = updated.replace(mount_anchor, &format!("{handoff_route}{mount_anchor}"));

    // 3. Mount the typed session-ingest router alongside the others.
    let use_anchor = "app.use(complianceRouter);";
    if !updated.contains(use_anchor) {
        bail!(
            "Could not find the router-mount anchor in {}; wire the relay by hand.",
            server_path.display()
        );
    }
    updated = updated.replace(use_anchor, &format!("{use_anchor}\napp.use(relayRouter);"));

    Ok(Some(RenderedTemplate {
        path: server_path,
        content: updated,
        context: None,
    }))
}

/// Adds the `INSTANCE_ID` / `INSTANCE_HMAC_KEY` environment config and the
/// `RelaySessionService` DI registration to the iam `registrations.ts`.
fn inject_relay_into_registrations_ts(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let registrations_path = iam_dir.join("registrations.ts");
    let content = read_to_string(&registrations_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&registrations_path)))?;

    if content.contains("RelaySessionService") {
        return Ok(None);
    }

    // 1. Imports for the service + its cookie-context type.
    let import_anchor = "import mikroOrmOptionsConfig from './mikro-orm.config';";
    if !content.contains(import_anchor) {
        bail!(
            "Could not find the import anchor in {}; wire the relay by hand.",
            registrations_path.display()
        );
    }
    let import_block = format!(
        "{import_anchor}\nimport {{\n  BetterAuthCookieContext,\n  RelaySessionService\n}} from './domain/services/relaySession.service';"
    );
    let mut updated = content.replace(import_anchor, &import_block);

    // 2. Environment config: INSTANCE_ID / INSTANCE_HMAC_KEY, inserted at the
    // end of the environmentConfig block (after the last known entry).
    if !updated.contains("INSTANCE_ID") {
        let env_anchor = "  JWKS_PUBLIC_KEY_URL: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('JWKS_PUBLIC_KEY_URL')\n  }\n});";
        if !updated.contains(env_anchor) {
            bail!(
                "Could not find the environmentConfig anchor in {}; wire the relay by hand.",
                registrations_path.display()
            );
        }
        let env_block = "  JWKS_PUBLIC_KEY_URL: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('JWKS_PUBLIC_KEY_URL')\n  },\n  INSTANCE_ID: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('INSTANCE_ID') ?? undefined\n  },\n  INSTANCE_HMAC_KEY: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('INSTANCE_HMAC_KEY') ?? undefined\n  }\n});";
        updated = updated.replace(env_anchor, env_block);
    }

    // 3. The RelaySessionService itself, appended to the last dependency chain
    // (expressApplicationOptions) so BetterAuth is available to its factory.
    let service_anchor = "  RetentionService: {\n    lifetime: Lifetime.Singleton,\n    type: RetentionService,\n    factory: ({ Orm, OtelCollector }) =>\n      new RetentionService(Orm, OtelCollector)\n  }\n});";
    if !updated.contains(service_anchor) {
        bail!(
            "Could not find the serviceDependencies anchor in {}; wire the relay by hand.",
            registrations_path.display()
        );
    }
    let service_block = "  RetentionService: {\n    lifetime: Lifetime.Singleton,\n    type: RetentionService,\n    factory: ({ Orm, OtelCollector }) =>\n      new RetentionService(Orm, OtelCollector)\n  },\n  RelaySessionService: {\n    lifetime: Lifetime.Scoped,\n    type: RelaySessionService,\n    factory: ({ EntityManager, BetterAuth, OtelCollector }) =>\n      new RelaySessionService(\n        EntityManager,\n        async (): Promise<BetterAuthCookieContext> => {\n          const ctx = (await (BetterAuth as BetterAuth).$context) as unknown as {\n            secret: string;\n            authCookies: {\n              sessionToken: {\n                name: string;\n                attributes: BetterAuthCookieContext['sessionTokenAttributes'];\n              };\n            };\n          };\n          return {\n            secret: ctx.secret,\n            sessionTokenName: ctx.authCookies.sessionToken.name,\n            sessionTokenAttributes: ctx.authCookies.sessionToken.attributes\n          };\n        },\n        OtelCollector\n      )\n  }\n});";
    updated = updated.replace(service_anchor, service_block);

    Ok(Some(RenderedTemplate {
        path: registrations_path,
        content: updated,
        context: None,
    }))
}

/// Re-exports the claim-mint controller from the controllers barrel.
///
/// `sdk.ts` imports from `./api/controllers`, the barrel — not from the
/// controller file directly — so a handler absent from it is invisible there
/// however correctly it is written. The relay's own controller never needed
/// this because nothing imports it through the barrel: its router reaches for
/// the file by path, and it is not in the sdk at all.
fn inject_claim_mint_into_controllers_index(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let index_path = iam_dir.join("api").join("controllers").join("index.ts");
    let content = read_to_string(&index_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&index_path)))?;

    if content.contains("claim-mint.controller") {
        return Ok(None);
    }

    let mut updated = content.trim_end().to_string();
    updated.push_str("\nexport * from './claim-mint.controller';\n");

    Ok(Some(RenderedTemplate {
        path: index_path,
        content: updated,
        context: None,
    }))
}

/// Registers the claim-mint handler in the service's `sdk.ts`.
///
/// This is the one injection the relay's own endpoint never needed, and it is
/// load-bearing. The framework walks `sdk.ts` to build each route's sdkPath
/// (`claim.mintClaimToken`), and that string becomes the route's `operationId`
/// in the OpenAPI document. The platform resolves its call through that
/// document — so an endpoint absent from `sdk.ts` exists, serves traffic, and
/// is nonetheless unreachable by the platform that is supposed to call it.
fn inject_claim_mint_into_sdk_ts(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let sdk_path = iam_dir.join("sdk.ts");
    let content = read_to_string(&sdk_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&sdk_path)))?;

    if content.contains("mintClaimToken") {
        return Ok(None);
    }

    let import_anchor = "
} from './api/controllers';";
    if !content.contains(import_anchor) {
        bail!(
            "Could not find the controller import block in {}; add `claim: {{ mintClaimToken }}` \
             to the sdk by hand, or the platform cannot resolve the claim mint.",
            sdk_path.display()
        );
    }
    // The comma matters: `import_anchor` closes the import list, so the new
    // name must be separated from the entry above it. Emitting the name alone
    // produced `surfaceRoles\n  mintClaimToken` — which reads fine and is a
    // TS1005 the moment the scaffolded app is built.
    let mut updated = content.replace(
        import_anchor,
        ",
  mintClaimToken
} from './api/controllers';",
    );

    // The type block and the value block are both `{ ... };`-terminated, so
    // anchor on each one's own closing line rather than guessing at names the
    // app may have changed.
    let type_anchor = "};

export const";
    if !updated.contains(type_anchor) {
        bail!(
            "Could not find the sdk type block in {}; add the claim group by hand.",
            sdk_path.display()
        );
    }
    updated = updated.replacen(
        type_anchor,
        "  claim: {
    mintClaimToken: typeof mintClaimToken;
  };
};

export const",
        1,
    );

    let value_anchor = "
} satisfies";
    if !updated.contains(value_anchor) {
        bail!(
            "Could not find the sdk value block in {}; add the claim group by hand.",
            sdk_path.display()
        );
    }
    updated = updated.replacen(
        value_anchor,
        ",
  claim: {
    mintClaimToken
  }
} satisfies",
        1,
    );

    Ok(Some(RenderedTemplate {
        path: sdk_path,
        content: updated,
        context: None,
    }))
}

/// Re-exports the handoff entity from the iam entities barrel so MikroORM
/// discovers it.
fn inject_relay_into_entities_index(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let index_path = iam_dir
        .join("persistence")
        .join("entities")
        .join("index.ts");
    let content = read_to_string(&index_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&index_path)))?;

    if content.contains("relaySessionHandoff.entity") {
        return Ok(None);
    }

    let export_line = "export { RelaySessionHandoff } from './relaySessionHandoff.entity';\n";
    let updated = format!("{content}{export_line}");

    Ok(Some(RenderedTemplate {
        path: index_path,
        content: updated,
        context: None,
    }))
}

fn print_next_steps(stdout: &mut StandardStream, is_better_auth: bool) -> Result<()> {
    log_header!(stdout, Color::Cyan, "Next steps:");
    writeln!(
        stdout,
        "  1. Fill in the one app-specific hook: iam/domain/hooks/relayHooks.ts"
    )?;
    writeln!(
        stdout,
        "     (store the relay tokens + resolve which user the session signs in)."
    )?;
    writeln!(
        stdout,
        "  2. Set INSTANCE_ID and INSTANCE_HMAC_KEY in the iam service env for managed mode"
    )?;
    writeln!(
        stdout,
        "     (they fall back to HMAC_SECRET_KEY/default for self-hosted)."
    )?;
    writeln!(
        stdout,
        "  3. Apply the migration (iam/migrations) or regenerate it for a non-postgres db."
    )?;
    if !is_better_auth {
        log_header!(stdout, Color::Yellow, "Heads up:");
        writeln!(
            stdout,
            "  Your iam service is not the better-auth variant. The session-cookie minting is"
        )?;
        writeln!(
            stdout,
            "  pre-wired for better-auth; on a base-iam service you must complete session"
        )?;
        writeln!(
            stdout,
            "  creation yourself in relaySession.service.ts / relayHooks.ts."
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, remove_dir_all, write};

    use super::*;
    use crate::core::rendered_template::TEMPLATES_DIR;

    fn embedded(path: &str) -> String {
        TEMPLATES_DIR
            .get_file(path)
            .unwrap_or_else(|| panic!("template {path} is not embedded"))
            .contents_utf8()
            .unwrap()
            .to_string()
    }

    /// The `init module -m relay` scaffold has to land the endpoint AND wire it
    /// in. This exercises the wiring against the REAL embedded iam-better-auth
    /// blueprint (not a hand-written stand-in), so if an anchor drifts in the
    /// blueprint this test fails instead of `init module -m relay` silently
    /// producing an unwired app.
    #[test]
    fn relay_wires_into_the_better_auth_iam_blueprint() {
        let tmp = std::env::temp_dir().join(format!("fl-relay-wire-{}", std::process::id()));
        let iam = tmp.join("iam");
        create_dir_all(iam.join("api").join("controllers")).unwrap();
        create_dir_all(iam.join("persistence").join("entities")).unwrap();
        write(
            iam.join("server.ts"),
            embedded("project/iam-better-auth/server.ts"),
        )
        .unwrap();
        write(
            iam.join("registrations.ts"),
            embedded("project/iam-better-auth/registrations.ts"),
        )
        .unwrap();
        write(
            iam.join("persistence").join("entities").join("index.ts"),
            embedded("project/iam-better-auth/persistence/entities/index.ts"),
        )
        .unwrap();

        let server = inject_relay_into_server_ts(&iam)
            .unwrap()
            .expect("server.ts must be wired");
        assert!(server.content.contains("import { relayRouter }"));
        assert!(server.content.contains("app.use(relayRouter);"));
        assert!(server.content.contains("app.internal.get('/relay/handoff'"));

        let registrations = inject_relay_into_registrations_ts(&iam)
            .unwrap()
            .expect("registrations.ts must be wired");
        assert!(registrations.content.contains("RelaySessionService"));
        assert!(registrations.content.contains("INSTANCE_ID"));
        assert!(registrations.content.contains("INSTANCE_HMAC_KEY"));

        let entities = inject_relay_into_entities_index(&iam)
            .unwrap()
            .expect("entities index must be wired");
        assert!(entities.content.contains("RelaySessionHandoff"));

        // Re-running must be a no-op once the anchors are already patched.
        write(iam.join("server.ts"), &server.content).unwrap();
        write(iam.join("registrations.ts"), &registrations.content).unwrap();
        write(
            iam.join("persistence").join("entities").join("index.ts"),
            &entities.content,
        )
        .unwrap();
        assert!(inject_relay_into_server_ts(&iam).unwrap().is_none());
        assert!(inject_relay_into_registrations_ts(&iam).unwrap().is_none());
        assert!(inject_relay_into_entities_index(&iam).unwrap().is_none());

        remove_dir_all(&tmp).ok();
    }

    /// The generated endpoint files must be embedded in the binary (otherwise
    /// the scaffold panics) and must carry the security-critical contract: a
    /// root-basePath session-ingest route with internal HMAC access.
    #[test]
    #[test]
    fn claim_mint_endpoint_template_is_embedded_and_purpose_checked() {
        let controller = embedded("project/relay/api/controllers/claim-mint.controller.ts");
        assert!(controller.contains("'/claim/mint'"));
        assert!(controller.contains("access: 'internal'"));
        // The platform's key is registered BESIDE the app's own, never
        // replacing it: an operator must still be able to mint by hand.
        assert!(controller.contains("default: HMAC_SECRET_KEY"));
        assert!(controller.contains("platform: INSTANCE_HMAC_KEY"));
        // One secret signs both directions, so a signature says what it is
        // for and this endpoint refuses any other purpose.
        assert!(controller.contains("app-claim-mint"));

        let routes = embedded("project/relay/api/routes/relay.routes.ts");
        assert!(routes.contains("'/claim/mint'"));

        let hooks = embedded("project/relay/domain/hooks/appClaimHooks.ts");
        assert!(hooks.contains("mintAppClaimLink"));
    }

    #[test]
    fn claim_mint_is_registered_in_the_service_sdk() {
        // The framework derives each route's operationId from its position in
        // sdk.ts, and the platform resolves its call through that operationId.
        // An endpoint missing here serves traffic and is still unreachable by
        // the platform, which is the failure this pins.
        let tmp = std::env::temp_dir().join(format!("fl-claim-sdk-{}", std::process::id()));
        let iam = tmp.join("iam");
        let _ = remove_dir_all(&tmp);
        create_dir_all(&iam).unwrap();
        // The REAL blueprint sdk.ts, not a hand-written stand-in. A toy
        // fixture is exactly what let a missing comma through: it had a
        // one-name import list, where the bug is invisible.
        write(
            iam.join("sdk.ts"),
            embedded("project/iam-better-auth/sdk.ts"),
        )
        .unwrap();

        // sdk.ts imports through the barrel, so the handler has to be
        // exported there too — an omission the compiler only reports from
        // sdk.ts, pointing at a line that looks correct.
        create_dir_all(iam.join("api").join("controllers")).unwrap();
        write(
            iam.join("api").join("controllers").join("index.ts"),
            embedded("project/iam-better-auth/api/controllers/index.ts"),
        )
        .unwrap();
        let barrel = inject_claim_mint_into_controllers_index(&iam)
            .unwrap()
            .expect("the controllers barrel must re-export the handler");
        assert!(
            barrel
                .content
                .contains("export * from './claim-mint.controller';")
        );
        write(
            iam.join("api").join("controllers").join("index.ts"),
            &barrel.content,
        )
        .unwrap();
        assert!(
            inject_claim_mint_into_controllers_index(&iam)
                .unwrap()
                .is_none(),
            "re-running must not export it twice"
        );

        let rendered = inject_claim_mint_into_sdk_ts(&iam).unwrap().unwrap();

        // The failure this pins is a MISSING COMMA, which earlier assertions
        // could not see: they checked that the new text was present, and it
        // was — in `surfaceRoles\n  mintClaimToken`, which parses as far as
        // a string search is concerned and is a TS1005 to a compiler. So
        // check separation, not presence, in all three blocks.
        assert!(
            rendered.content.contains("surfaceRoles,\n  mintClaimToken"),
            "import list must separate the new name: {}",
            rendered.content
        );
        // The value block's previous entry needs the same separator.
        assert!(
            rendered
                .content
                .contains("},\n  claim: {\n    mintClaimToken\n  }"),
            "sdk value block must separate the claim group: {}",
            rendered.content
        );
        assert!(
            !rendered.content.contains("}\n  ,"),
            "a comma on its own line means the anchor matched a closing brace: {}",
            rendered.content
        );
        // Every `{`/`}` still balances — a crude parse, but it catches an
        // injection that lands inside the wrong block.
        assert_eq!(
            rendered.content.matches('{').count(),
            rendered.content.matches('}').count(),
            "braces must balance: {}",
            rendered.content
        );
        assert!(
            rendered
                .content
                .contains("mintClaimToken: typeof mintClaimToken;")
        );
        assert!(rendered.content.contains("claim: {"));

        // Re-running the installer must not double-register.
        write(iam.join("sdk.ts"), &rendered.content).unwrap();
        assert!(inject_claim_mint_into_sdk_ts(&iam).unwrap().is_none());

        let _ = remove_dir_all(&tmp);
    }

    #[test]
    fn relay_endpoint_template_is_embedded() {
        assert!(
            TEMPLATES_DIR.get_dir("project/relay").is_some(),
            "project/relay template dir is not embedded"
        );
        let controller = embedded("project/relay/api/controllers/relay.controller.ts");
        assert!(controller.contains("'/relay/session-ingest'"));
        assert!(controller.contains("access: 'internal'"));

        let routes = embedded("project/relay/api/routes/relay.routes.ts");
        // Root basePath so the HMAC-verified req.path is the full signed path.
        assert!(routes.contains("forklaunchRouter(\n  '/'"));

        // The single app-specific hook is present and named as documented.
        let hook = embedded("project/relay/domain/hooks/relayHooks.ts");
        assert!(hook.contains("establishSessionFromRelayTokens"));
    }
}
