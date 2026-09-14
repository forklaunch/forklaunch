//! Managed-apps OAuth relay session-ingest module.
//!
//! Unlike every other `Module`, relay does not scaffold a new service. It
//! injects the instance-side `/relay/session-ingest` endpoint (and its
//! browser-facing `/relay/handoff` redirect) into the app's EXISTING iam
//! service - the same shape Health Vault hand-built for managed-apps
//! readiness, generalized so the only app-specific decision is one hook.
//!
//! The generic ~80% it writes: the HMAC-verified ingest controller + route, the
//! nonce single-use replay guard (a unique-column handoff entity, whose
//! migration the app generates via `migrate:init`/`migrate:create`),
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
use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, SourceType, Statement};
use oxc_codegen::{Codegen, CodegenOptions};
use termcolor::{Color, StandardStream, WriteColor};

use crate::{
    constants::{Database, Module, error_failed_to_read_file},
    core::{
        ast::{
            injections::{
                inject_into_import_statement::inject_into_import_statement,
                inject_into_registrations_ts::{
                    find_config_injector_chain_names, inject_into_registrations_config_injector,
                },
                inject_into_server_ts::inject_into_server_ts,
            },
            parse_ast_program::parse_ast_program,
        },
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

    write_rendered_templates(&rendered_templates, dryrun, stdout)?;

    if !dryrun {
        format_code(base_path, &service_data.runtime.parse()?);
        log_ok!(
            stdout,
            "relay session-ingest endpoint injected into the {AUTH_SERVICE_NAME} service"
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
///
/// Wiring is STRUCTURAL, not verbatim-string-matched. It reuses the same
/// `inject_into_server_ts` / `inject_into_import_statement` AST machinery the
/// normal `init module` router flow uses (`transform_server_ts`): the router is
/// mounted after the run of `app.use(...)` calls and the handoff route is placed
/// just before the first one, so any conformant iam wires - not only the pristine
/// blueprint whose last mount happens to be `app.use(complianceRouter)`.
fn inject_relay_into_server_ts(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let server_path = iam_dir.join("server.ts");
    let content = read_to_string(&server_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&server_path)))?;

    if content.contains("relayRouter") {
        return Ok(None);
    }

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(&server_path)?;
    let mut server_program = parse_ast_program(&allocator, &content, source_type);

    // 1. The browser-facing handoff redirect, placed just before the routes are
    // mounted so it wins over any catch-all. Inserted at the position of the
    // first `app.use(...)` (i.e. immediately before the mount run).
    let handoff_text = r#"app.internal.get('/relay/handoff', async (req, res) => {
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
});"#;
    let mut handoff_injection = parse_ast_program(&allocator, handoff_text, source_type);
    inject_into_server_ts(&mut server_program, &mut handoff_injection, |statements| {
        find_first_app_use_index(statements)
    })
    .map_err(|_| {
        anyhow::anyhow!(
            "Could not find any `app.use(...)` route mounts in {}; there is nothing to mount the \
             relay alongside. Wire it by hand following the module docs.",
            server_path.display()
        )
    })?;

    // 2. Mount the typed session-ingest router after the run of `app.use(...)`
    // calls, wherever that run is.
    let use_text = "app.use(relayRouter);";
    let mut use_injection = parse_ast_program(&allocator, use_text, source_type);
    inject_into_server_ts(&mut server_program, &mut use_injection, |statements| {
        find_after_last_app_use_index(statements)
    })
    .map_err(|_| {
        anyhow::anyhow!(
            "Could not find any `app.use(...)` route mounts in {}; there is nothing to mount the \
             relay alongside. Wire it by hand following the module docs.",
            server_path.display()
        )
    })?;

    // 3. Imports for the router + the cookie serializer. `inject_into_import_statement`
    // slots each new import in among the existing ones structurally.
    let router_import_text = "import { relayRouter } from './api/routes/relay.routes';";
    let mut router_import = parse_ast_program(&allocator, router_import_text, source_type);
    inject_into_import_statement(
        &mut server_program,
        &mut router_import,
        "./api/routes/relay.routes",
        &content,
    )?;

    let serializer_import_text =
        "import { serializeSessionCookie } from './domain/services/relaySession.service';";
    let mut serializer_import = parse_ast_program(&allocator, serializer_import_text, source_type);
    inject_into_import_statement(
        &mut server_program,
        &mut serializer_import,
        "./domain/services/relaySession.service",
        &content,
    )?;

    let updated = Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&server_program)
        .code;

    Ok(Some(RenderedTemplate {
        path: server_path,
        content: updated,
        context: None,
    }))
}

/// Index of the first top-level `app.use(...)` call statement, for inserting the
/// handoff route just ahead of the route-mount run.
fn find_first_app_use_index(statements: &oxc_allocator::Vec<'_, Statement<'_>>) -> Option<usize> {
    statements.iter().enumerate().find_map(|(index, stmt)| {
        if is_app_use_statement(stmt) {
            Some(index)
        } else {
            None
        }
    })
}

/// Index just after the last top-level `app.use(...)` call statement, mirroring
/// the closure `transform_server_ts` uses to append a router mount.
fn find_after_last_app_use_index(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
) -> Option<usize> {
    let mut splice_pos = None;
    for (index, stmt) in statements.iter().enumerate() {
        if is_app_use_statement(stmt) {
            splice_pos = Some(index + 1);
        }
    }
    splice_pos
}

/// Whether a statement is an `app.use(...)` call expression.
fn is_app_use_statement(stmt: &Statement<'_>) -> bool {
    let Statement::ExpressionStatement(expr) = stmt else {
        return false;
    };
    let Expression::CallExpression(call) = &expr.expression else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(id) = &member.object else {
        return false;
    };
    id.name == "app" && member.property.name == "use"
}

/// Adds the `INSTANCE_ID` / `INSTANCE_HMAC_KEY` environment config and the
/// `RelaySessionService` DI registration to the iam `registrations.ts`.
///
/// Wiring is STRUCTURAL. It reuses the same `inject_into_registrations_config_injector`
/// AST machinery `transform_registrations_ts` uses: it locates the config-injector
/// `.chain({ … })` calls by shape (via `find_config_injector_chain_names`) and
/// appends entries to them, instead of matching a specific sibling entry's
/// verbatim text. The env vars land in the environment chain; `RelaySessionService`
/// lands in the terminal chain (the last `.chain(...)`, where `BetterAuth` is
/// registered and thus in scope for its factory) - exactly where the blueprint
/// puts it. A customized iam whose entries have drifted (e.g. a last service that
/// is not `RetentionService`) still wires.
fn inject_relay_into_registrations_ts(iam_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let registrations_path = iam_dir.join("registrations.ts");
    let content = read_to_string(&registrations_path)
        .map_err(|_| anyhow::anyhow!(error_failed_to_read_file(&registrations_path)))?;

    if content.contains("RelaySessionService") {
        return Ok(None);
    }

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(&registrations_path)?;
    let mut registrations_program = parse_ast_program(&allocator, &content, source_type);

    // Locate the config-injector chains structurally. The environment vars land
    // in the environment chain; the service lands in the terminal chain so the
    // BetterAuth registered there is in scope for its factory.
    let chain_names = find_config_injector_chain_names(&registrations_program);
    let Some(service_chain) = chain_names.last().cloned() else {
        bail!(
            "Could not find any config-injector `.chain({{ … }})` in {}; this does not look like a \
             ForkLaunch registrations file. Wire the relay by hand following the module docs.",
            registrations_path.display()
        );
    };
    // Prefer the conventional `environmentConfig` chain; fall back to the first
    // chain when a customized iam has renamed it.
    let env_chain = chain_names
        .iter()
        .find(|name| name.as_str() == "environmentConfig")
        .cloned()
        .or_else(|| chain_names.first().cloned())
        .expect("chain_names is non-empty (service_chain was Some)");

    // 1. Imports for the service + its cookie-context type.
    let import_text = "import { BetterAuthCookieContext, RelaySessionService } from './domain/services/relaySession.service';";
    let mut import_injection = parse_ast_program(&allocator, import_text, source_type);
    inject_into_import_statement(
        &mut registrations_program,
        &mut import_injection,
        "./domain/services/relaySession.service",
        &content,
    )?;

    // 2. Environment config: INSTANCE_ID / INSTANCE_HMAC_KEY, appended to the
    // environment chain.
    let env_injection_text = format!(
        "const {env_chain} = configInjector.chain({{
  INSTANCE_ID: {{
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('INSTANCE_ID') ?? undefined
  }},
  INSTANCE_HMAC_KEY: {{
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('INSTANCE_HMAC_KEY') ?? undefined
  }}
}});"
    );
    let mut env_injection = parse_ast_program(&allocator, &env_injection_text, source_type);
    inject_into_registrations_config_injector(
        &allocator,
        &mut registrations_program,
        &mut env_injection,
        &env_chain,
    )?;

    // 3. The RelaySessionService itself, appended to the terminal chain so
    // BetterAuth is available to its factory.
    let service_injection_text = format!(
        "const {service_chain} = serviceDependencies.chain({{
  RelaySessionService: {{
    lifetime: Lifetime.Scoped,
    type: RelaySessionService,
    factory: ({{ EntityManager, BetterAuth, OtelCollector }}) =>
      new RelaySessionService(
        EntityManager,
        async (): Promise<BetterAuthCookieContext> => {{
          const ctx = (await (BetterAuth as BetterAuth).$context) as unknown as {{
            secret: string;
            authCookies: {{
              sessionToken: {{
                name: string;
                attributes: BetterAuthCookieContext['sessionTokenAttributes'];
              }};
            }};
          }};
          return {{
            secret: ctx.secret,
            sessionTokenName: ctx.authCookies.sessionToken.name,
            sessionTokenAttributes: ctx.authCookies.sessionToken.attributes
          }};
        }},
        OtelCollector
      )
  }}
}});"
    );
    let mut service_injection = parse_ast_program(&allocator, &service_injection_text, source_type);
    inject_into_registrations_config_injector(
        &allocator,
        &mut registrations_program,
        &mut service_injection,
        &service_chain,
    )?;

    let updated = Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code;

    // The chains were located structurally, so injection should always land;
    // guard against a silently-unwired file rather than emitting a broken app.
    if !updated.contains("RelaySessionService") || !updated.contains("INSTANCE_ID") {
        bail!(
            "Failed to wire the relay into {} (the config-injector chains were found but the \
             entries did not inject). Wire the relay by hand following the module docs.",
            registrations_path.display()
        );
    }

    Ok(Some(RenderedTemplate {
        path: registrations_path,
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
        "  3. Generate + apply the handoff-table migration: from iam/, run `migrate:create`"
    )?;
    writeln!(
        stdout,
        "     then `migrate:up` (a fresh scaffold's `database:setup` does this for you)."
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
        // Codegen normalizes quotes, so match the path without asserting a quote style.
        assert!(
            server
                .content
                .contains("app.internal.get(\"/relay/handoff\"")
        );
        // The router mount joins the existing run of `app.use(...)` calls.
        assert!(server.content.contains("app.use(complianceRouter);"));

        let registrations = inject_relay_into_registrations_ts(&iam)
            .unwrap()
            .expect("registrations.ts must be wired");
        assert!(registrations.content.contains("RelaySessionService"));
        assert!(registrations.content.contains("INSTANCE_ID"));
        assert!(registrations.content.contains("INSTANCE_HMAC_KEY"));
        // The service lands in the terminal chain, as a sibling of BetterAuth.
        assert!(registrations.content.contains("BetterAuthCookieContext"));

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

    /// The Health-Vault-shaped case: a customized iam whose wiring has DRIFTED
    /// from the pristine blueprint. Here the terminal chain's last service is
    /// renamed away from `RetentionService`, an EXTRA service is added, the last
    /// mounted router is not `complianceRouter`, and the env chain's last entry
    /// is not `JWKS_PUBLIC_KEY_URL`. The old verbatim-string anchors would bail
    /// on every one of these; the structural wiring must still land.
    #[test]
    fn relay_wires_into_a_drifted_iam() {
        let mut server = embedded("project/iam-better-auth/server.ts");
        // Drift the mount run: rename complianceRouter -> auditRouter and drop the
        // verbatim `app.use(complianceRouter);` anchor entirely, appending an
        // extra mount so the last `app.use` is a fresh name.
        server = server.replace("complianceRouter", "auditRouter").replace(
            "app.use(auditRouter);",
            "app.use(auditRouter);\napp.use(webhookRouter);",
        );
        assert!(!server.contains("app.use(complianceRouter);"));

        let mut registrations = embedded("project/iam-better-auth/registrations.ts");
        // Drift the terminal chain: rename RetentionService (the old service
        // anchor) and add an extra service after it.
        registrations = registrations.replace(
            "RetentionService: {\n    lifetime: Lifetime.Singleton,\n    type: RetentionService,\n    factory: ({ Orm, OtelCollector }) =>\n      new RetentionService(Orm, OtelCollector)\n  }",
            "DataRetentionService: {\n    lifetime: Lifetime.Singleton,\n    type: RetentionService,\n    factory: ({ Orm, OtelCollector }) =>\n      new RetentionService(Orm, OtelCollector)\n  },\n  AnalyticsService: {\n    lifetime: Lifetime.Scoped,\n    type: SurfacingService,\n    factory: ({ EntityManager }) => new SurfacingService(EntityManager)\n  }",
        );
        // Drift the env chain: append a custom env var after JWKS_PUBLIC_KEY_URL so
        // the old env anchor (which required JWKS to be the LAST entry) is gone.
        registrations = registrations.replace(
            "  JWKS_PUBLIC_KEY_URL: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('JWKS_PUBLIC_KEY_URL')\n  }\n});",
            "  JWKS_PUBLIC_KEY_URL: {\n    lifetime: Lifetime.Singleton,\n    type: string,\n    value: getEnvVar('JWKS_PUBLIC_KEY_URL')\n  },\n  TENANT_HEADER: {\n    lifetime: Lifetime.Singleton,\n    type: optional(string),\n    value: getEnvVar('TENANT_HEADER') ?? undefined\n  }\n});",
        );
        // The exact block the old verbatim wiring anchored on is now gone.
        assert!(!registrations.contains(
            "  RetentionService: {\n    lifetime: Lifetime.Singleton,\n    type: RetentionService,\n    factory: ({ Orm, OtelCollector }) =>\n      new RetentionService(Orm, OtelCollector)\n  }\n});"
        ));

        let tmp = std::env::temp_dir().join(format!("fl-relay-drift-{}", std::process::id()));
        let iam = tmp.join("iam");
        create_dir_all(iam.join("api").join("controllers")).unwrap();
        create_dir_all(iam.join("persistence").join("entities")).unwrap();
        write(iam.join("server.ts"), &server).unwrap();
        write(iam.join("registrations.ts"), &registrations).unwrap();
        write(
            iam.join("persistence").join("entities").join("index.ts"),
            embedded("project/iam-better-auth/persistence/entities/index.ts"),
        )
        .unwrap();

        // Despite none of the old verbatim anchors being present, the relay wires.
        let server_out = inject_relay_into_server_ts(&iam)
            .unwrap()
            .expect("drifted server.ts must still be wired");
        assert!(server_out.content.contains("app.use(relayRouter);"));
        assert!(server_out.content.contains("import { relayRouter }"));
        assert!(
            server_out
                .content
                .contains("app.internal.get(\"/relay/handoff\"")
        );
        // The pre-existing (drifted) mounts survive.
        assert!(server_out.content.contains("app.use(auditRouter);"));
        assert!(server_out.content.contains("app.use(webhookRouter);"));

        let registrations_out = inject_relay_into_registrations_ts(&iam)
            .unwrap()
            .expect("drifted registrations.ts must still be wired");
        assert!(registrations_out.content.contains("RelaySessionService"));
        assert!(registrations_out.content.contains("INSTANCE_ID"));
        assert!(registrations_out.content.contains("INSTANCE_HMAC_KEY"));
        // The drifted siblings survive alongside the injected entry.
        assert!(registrations_out.content.contains("DataRetentionService"));
        assert!(registrations_out.content.contains("AnalyticsService"));
        assert!(registrations_out.content.contains("TENANT_HEADER"));

        remove_dir_all(&tmp).ok();
    }

    /// The generated endpoint files must be embedded in the binary (otherwise
    /// the scaffold panics) and must carry the security-critical contract: a
    /// root-basePath session-ingest route with internal HMAC access.
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
