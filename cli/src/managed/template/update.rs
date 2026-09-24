use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, patch_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::{AppTemplate, CLUSTER_TYPES, TEMPLATE_STATUSES, parse_app_claim_hook},
    },
};

/// The fields `PATCH /managed-mode/templates/:slug` accepts, plus the two output flags.
///
/// Passed explicitly rather than by handing the shared code an `ArgMatches`, because
/// `publish-template` deliberately does not define `--name` / `--description` /
/// `--status`, and `ArgMatches::get_one` panics on an argument id the command never
/// declared.
#[derive(Debug, Default)]
pub(super) struct TemplateUpdate<'a> {
    pub(super) name: Option<&'a String>,
    pub(super) description: Option<&'a String>,
    pub(super) status: Option<&'a str>,
    pub(super) stripe_product: Option<&'a String>,
    pub(super) cluster_type: Option<&'a str>,
    pub(super) base_domain: Option<&'a String>,
    /// `Some(None)` clears the frontend domain (`--clear-frontend-domain`).
    pub(super) frontend_domain: Option<Option<&'a String>>,
    /// `Some(None)` returns to the platform default (`--clear-default-instance-size`).
    pub(super) default_instance_size: Option<Option<&'a String>>,
    /// Whether the platform may rotate this product's generated secrets in place
    /// (`--supports-key-rotation` / `--no-supports-key-rotation`).
    pub(super) supports_key_rotation: Option<bool>,
    /// The repository the platform builds versions from (https URL).
    pub(super) source_repo: Option<&'a String>,
    /// `<component>:<path>` — where this product mints its own claim link.
    pub(super) app_claim_hook: Option<&'a String>,
    pub(super) dryrun: bool,
    pub(super) json: bool,
}

#[derive(Debug)]
pub(super) struct UpdateCommand;

impl UpdateCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for UpdateCommand {
    fn command(&self) -> Command {
        command(
            "update",
            "Change a template's name, description, status, Stripe product, placement, or domains",
        )
        .long_about(
            "Change a template's name, description, status, or Stripe product.\n\n\
             Only the fields you pass are changed; everything else is left alone.\n\n\
             --status is the important one. A template is created as `draft`, and\n\
             `instance create` will only launch from a `published` template, so a template\n\
             stays uninstantiable until its status is moved. `template publish-template` is\n\
             the shorthand for exactly that, and is usually what you want.\n\n\
             The three statuses:\n\
             \x20 draft      registered but not launchable — the state every template starts in\n\
             \x20 published  launchable; `instance create` accepts it\n\
             \x20 retired    no longer launchable for new instances\n\n\
             --stripe-product records a Stripe product id against the template. Note that\n\
             nothing in billing reads that id today, so setting it does not by itself cause\n\
             anyone to be charged — it is stored for later use.\n\n\
             Placement and addressing (the same fields the dashboard edits):\n\
             \x20 --cluster-type           where NEW instances run: org-shared (default),\n\
             \x20                          platform-shared, or dedicated. Existing instances\n\
             \x20                          do not move; a component's manifest hostingType wins.\n\
             \x20 --base-domain            the zone instances are hosted under (their API hosts).\n\
             \x20 --frontend-domain        the domain the product UI is served from; each\n\
             \x20                          instance's UI becomes <hostPrefix>.<frontend domain>\n\
             \x20                          and the claim page shows customers that link.\n\
             \x20 --default-instance-size  the compute tier new instances launch with\n\
             \x20                          (pico, nano, micro, small, ...).\n\
             \x20 --supports-key-rotation  declare that every service re-encrypts its data on\n\
             \x20                          boot from LEGACY_<KEY>S, so `instance rotate-keys`\n\
             \x20                          is allowed; --no-supports-key-rotation withdraws it.\n\
             \x20 --source-repo            the repository versions are built from. Re-point it\n\
             \x20                          when the code moves (e.g. to the customer's account);\n\
             \x20                          the org's GitHub App installation must read it.\n\n\
             --app-claim-hook tells the platform where this product mints its OWN sign-up\n\
             link, as <component>:<path> — for example `iam:/internal/claim/mint`. With it\n\
             set, the moment a customer claims an instance the platform calls that endpoint\n\
             over the service mesh and holds what comes back, so the customer gets a second\n\
             link that creates their first account INSIDE the app. The component must be a\n\
             service the built app contains and the path one it serves; a version whose\n\
             build does not match is refused at publish time rather than discovered by the\n\
             first customer who claims.\n\n\
             To stop asking the product for a link, use `template clear-app-claim-hook`. It\n\
             is a separate command rather than an empty --app-claim-hook because \"set it to\n\
             nothing\" and \"do not change it\" are the same empty string on a command line.",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template to update"),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .help("New human-readable template name"),
        )
        .arg(
            Arg::new("description")
                .long("description")
                .help("New description shown alongside the template"),
        )
        .arg(
            Arg::new("status")
                .long("status")
                .value_parser(TEMPLATE_STATUSES.to_vec())
                .help("New status — `published` is what makes the template launchable"),
        )
        .arg(
            Arg::new("stripe_product")
                .long("stripe-product")
                .help("Stripe product id to record against the template (not yet read by billing)"),
        )
        .arg(
            Arg::new("cluster_type")
                .long("cluster-type")
                .value_parser(CLUSTER_TYPES.to_vec())
                .help("Where NEW instances run: org-shared | platform-shared | dedicated"),
        )
        .arg(
            Arg::new("base_domain")
                .long("base-domain")
                .help("The zone instances are hosted under, e.g. buildbespoke.app"),
        )
        .arg(
            Arg::new("frontend_domain")
                .long("frontend-domain")
                .conflicts_with("clear_frontend_domain")
                .help("Domain the product UI is served from; instance UIs become <hostPrefix>.<domain>"),
        )
        .arg(
            Arg::new("clear_frontend_domain")
                .long("clear-frontend-domain")
                .help("Remove the frontend domain (instances stop advertising a UI link)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("default_instance_size")
                .long("default-instance-size")
                .conflicts_with("clear_default_instance_size")
                .help("Compute tier new instances launch with (pico, nano, micro, small, ...)"),
        )
        .arg(
            Arg::new("clear_default_instance_size")
                .long("clear-default-instance-size")
                .help("Return new instances to the platform's default tier")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("supports_key_rotation")
                .long("supports-key-rotation")
                .conflicts_with("no_supports_key_rotation")
                .help("Allow `instance rotate-keys`: every service re-encrypts on boot from LEGACY_<KEY>S")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_supports_key_rotation")
                .long("no-supports-key-rotation")
                .help("Refuse `instance rotate-keys` for this product")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("source_repo")
                .long("source-repo")
                .help("https URL of the repository versions are built from, e.g. https://github.com/org/repo"),
        )
        .arg(
            Arg::new("app_claim_hook")
                .long("app-claim-hook")
                .value_name("COMPONENT:PATH")
                .help("Where this product mints its own sign-up link, e.g. iam:/internal/claim/mint"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;

        if let Some(repo) = matches.get_one::<String>("source_repo") {
            if !repo.starts_with("https://") || repo.trim_end_matches('/').matches('/').count() != 4 {
                bail!(
                    "--source-repo '{}' must be an https repository URL such as https://github.com/org/repo",
                    repo
                );
            }
        }

        let supports_key_rotation = if matches.get_flag("supports_key_rotation") {
            Some(true)
        } else if matches.get_flag("no_supports_key_rotation") {
            Some(false)
        } else {
            None
        };
        let frontend_domain = if matches.get_flag("clear_frontend_domain") {
            Some(None)
        } else {
            matches.get_one::<String>("frontend_domain").map(Some)
        };
        let default_instance_size = if matches.get_flag("clear_default_instance_size") {
            Some(None)
        } else {
            matches.get_one::<String>("default_instance_size").map(Some)
        };

        update_template(
            slug,
            TemplateUpdate {
                name: matches.get_one::<String>("name"),
                description: matches.get_one::<String>("description"),
                status: matches.get_one::<String>("status").map(String::as_str),
                stripe_product: matches.get_one::<String>("stripe_product"),
                cluster_type: matches
                    .get_one::<String>("cluster_type")
                    .map(String::as_str),
                base_domain: matches.get_one::<String>("base_domain"),
                frontend_domain,
                default_instance_size,
                supports_key_rotation,
                source_repo: matches.get_one::<String>("source_repo"),
                app_claim_hook: matches.get_one::<String>("app_claim_hook"),
                dryrun: matches.get_flag("dryrun"),
                json: matches.get_flag("json"),
            },
        )
    }
}

/// Issues the PATCH. Shared by `template update` and `template publish-template`.
pub(super) fn update_template(slug: &str, update: TemplateUpdate<'_>) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    let mut body = Map::new();
    if let Some(name) = update.name {
        body.insert("name".to_string(), json!(name));
    }
    if let Some(description) = update.description {
        body.insert("description".to_string(), json!(description));
    }
    if let Some(status) = update.status {
        body.insert("status".to_string(), json!(status));
    }
    if let Some(stripe_product) = update.stripe_product {
        body.insert("stripeProductId".to_string(), json!(stripe_product));
    }
    if let Some(cluster_type) = update.cluster_type {
        body.insert("clusterType".to_string(), json!(cluster_type));
    }
    if let Some(base_domain) = update.base_domain {
        body.insert("baseDomain".to_string(), json!(base_domain));
    }
    // `null` is a real value here: it tells the control plane to clear the field.
    if let Some(frontend_domain) = update.frontend_domain {
        body.insert("frontendDomain".to_string(), json!(frontend_domain));
    }
    if let Some(default_instance_size) = update.default_instance_size {
        body.insert(
            "defaultInstanceSize".to_string(),
            json!(default_instance_size),
        );
    }
    if let Some(supports_key_rotation) = update.supports_key_rotation {
        body.insert(
            "supportsKeyRotation".to_string(),
            json!(supports_key_rotation),
        );
    }
    if let Some(source_repo) = update.source_repo {
        body.insert("sourceRepo".to_string(), json!(source_repo));
    }
    if let Some(raw) = update.app_claim_hook {
        // Parsed before anything is sent, so a malformed pair is a local error naming the
        // form rather than a 400 whose body has to be read back through two services.
        let hook = parse_app_claim_hook(raw).map_err(|message| anyhow::anyhow!(message))?;
        body.insert("appClaimHook".to_string(), json!(hook));
    }

    // An empty PATCH is accepted by the control plane and changes nothing, so it would
    // report success while having done nothing at all. Refuse instead — someone who
    // typed `template update --slug x` and saw "Updated" would reasonably believe
    // something had happened.
    if body.is_empty() {
        bail!(
            "nothing to update — pass at least one of --name, --description, --status, \
             --stripe-product, --cluster-type, --base-domain, --frontend-domain, \
             --default-instance-size, --supports-key-rotation, --source-repo, or \
             --app-claim-hook (to publish a template, `forklaunch managed template \
             publish-template --slug {}` is the shorthand)",
            slug
        );
    }

    let body = Value::Object(body);
    // Slugs are organization-authored identifiers, not free-form user input, but they
    // still land in a URL path — encode so a slug containing a separator cannot
    // restructure the request.
    let path = format!("/templates/{}", urlencoding::encode(slug));

    if update.dryrun {
        return print_dryrun("PATCH", &path, Some(&body));
    }

    let auth_mode = resolve_managed_auth()?;
    require_managed_mode(&auth_mode)?;

    let template: AppTemplate = patch_json(
        &path,
        body,
        Missing::Resource(format!("template '{}'", slug)),
    )?;

    if update.json {
        println!("{}", serde_json::to_string_pretty(&template)?);
        return Ok(());
    }

    // The control plane echoes the resulting status back; fall back to what was asked
    // for only if it did not.
    let new_status = template
        .status
        .as_deref()
        .or(update.status)
        .unwrap_or("unchanged");

    log_ok!(
        stdout,
        "Updated template '{}' — status: {}",
        slug,
        new_status
    );

    if let Some(Some(domain)) = update.frontend_domain {
        log_info!(
            stdout,
            "Each instance's UI is now https://<hostPrefix>.{} — point that wildcard at your frontend deployment (see the vercel-frontend skill).",
            domain
        );
    }

    // Echoed back from the server's own response rather than from what was asked for.
    // Confirming the round trip is the whole reason the control plane was changed to
    // return this field.
    if update.app_claim_hook.is_some() {
        match template.app_claim_hook.as_ref() {
            Some(hook) => log_ok!(
                stdout,
                "App claim hook: {}{} — on claim, the platform will ask that endpoint for the customer's sign-up link.",
                hook.component,
                hook.path
            ),
            // A 200 that does not echo the field means an older control plane, not a
            // failed write; say which, so nobody re-runs the PATCH chasing a ghost.
            None => log_warn!(
                stdout,
                "This control plane did not echo the app claim hook back, so it could not be confirmed here. Read it with `template list --json`."
            ),
        }
    }

    if new_status == "published" {
        log_info!(
            stdout,
            "Instances can now be launched from it: forklaunch managed instance create --template {} --region <region>",
            slug
        );
    } else if new_status == "draft" {
        log_info!(
            stdout,
            "This template is still a draft, so `instance create` will refuse it. Publish it with: forklaunch managed template publish-template --slug {}",
            slug
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_with_no_fields_is_refused_rather_than_reported_as_success() {
        // Reaches the guard before any network or auth: an empty PATCH is accepted by
        // the control plane and changes nothing, so succeeding here would be a lie.
        let error = update_template("clinic", TemplateUpdate::default()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("nothing to update"), "{}", message);
        assert!(message.contains("publish-template"), "{}", message);
        // Every settable field belongs in the list, or someone is told a flag they just
        // used does not exist.
        assert!(message.contains("--app-claim-hook"), "{}", message);
    }

    #[test]
    fn a_malformed_app_claim_hook_fails_before_any_request_is_made() {
        // `dryrun` would short-circuit the network anyway; what this pins is that the
        // parse happens FIRST, so the error names the form rather than arriving as a 400
        // relayed through two services.
        let raw = "iam".to_string();
        let error = update_template(
            "clinic",
            TemplateUpdate {
                app_claim_hook: Some(&raw),
                dryrun: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("<component>:<path>"), "{}", error);
    }

    #[test]
    fn publish_template_sends_exactly_a_published_status() {
        // What `publish-template` forwards, asserted without a server: status only, and
        // none of the other patchable fields silently along for the ride.
        let update = TemplateUpdate {
            status: Some("published"),
            dryrun: true,
            ..Default::default()
        };
        assert_eq!(update.status, Some("published"));
        assert!(update.name.is_none());
        assert!(update.description.is_none());
        assert!(update.stripe_product.is_none());
        assert!(update.frontend_domain.is_none());
    }

    #[test]
    fn clearing_the_frontend_domain_sends_an_explicit_null() {
        // `--clear-frontend-domain` is Some(None): the field must be PRESENT as null so
        // the control plane clears it — omitting it would leave the domain untouched.
        let update = TemplateUpdate {
            frontend_domain: Some(None),
            ..Default::default()
        };
        assert_eq!(json!(update.frontend_domain.unwrap()), Value::Null);
    }

    #[test]
    fn the_cli_cluster_type_list_matches_what_the_control_plane_validates() {
        assert_eq!(
            CLUSTER_TYPES,
            &["org-shared", "platform-shared", "dedicated"]
        );
    }

    #[test]
    fn the_cli_status_list_matches_what_the_control_plane_validates() {
        // The control plane answers 400 with exactly this list. If the two drift, the
        // CLI would reject a status the server accepts (or vice versa).
        assert_eq!(TEMPLATE_STATUSES, &["draft", "published", "retired"]);
    }
}
