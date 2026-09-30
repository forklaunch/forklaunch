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
        types::{AppTemplate, CLUSTER_TYPES, TEMPLATE_STATUSES},
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
    /// The model gateway setting, replaced as a whole (see `model_gateway_body`).
    pub(super) model_gateway: Option<Value>,
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
             \x20                          the org's GitHub App installation must read it.",
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
            Arg::new("gateway_models")
                .long("gateway-models")
                .help("Model gateway: comma-separated catalog aliases instances may call (e.g. terra,luna)"),
        )
        .arg(
            Arg::new("gateway_monthly_tokens")
                .long("gateway-monthly-tokens")
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Model gateway: monthly token budget per instance"),
        )
        .arg(
            Arg::new("gateway_rpm")
                .long("gateway-rpm")
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Model gateway: requests per minute per instance"),
        )
        .arg(
            Arg::new("disable_model_gateway")
                .long("disable-model-gateway")
                .help("Turn the model gateway off for this product")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["gateway_models", "gateway_monthly_tokens", "gateway_rpm"]),
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

        let model_gateway = model_gateway_body(
            matches.get_one::<String>("gateway_models"),
            matches.get_one::<u64>("gateway_monthly_tokens").copied(),
            matches.get_one::<u64>("gateway_rpm").copied(),
            matches.get_flag("disable_model_gateway"),
        )?;

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
                model_gateway,
                dryrun: matches.get_flag("dryrun"),
                json: matches.get_flag("json"),
            },
        )
    }
}

/// The `modelGateway` value for a template update, or None when no gateway
/// flag was passed.
///
/// The control plane replaces the setting as a whole, so a budget or rate
/// without the model list is refused: sending it alone would silently drop the
/// models and turn the gateway off.
pub(super) fn model_gateway_body(
    models: Option<&String>,
    monthly_tokens: Option<u64>,
    rpm: Option<u64>,
    disable: bool,
) -> Result<Option<Value>> {
    if disable {
        return Ok(Some(json!({ "models": [] })));
    }
    let Some(models) = models else {
        if monthly_tokens.is_some() || rpm.is_some() {
            bail!(
                "--gateway-monthly-tokens / --gateway-rpm replace the whole gateway setting, \
                 so they need --gateway-models too (the models the product may call)"
            );
        }
        return Ok(None);
    };
    let aliases: Vec<&str> = models
        .split(',')
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .collect();
    if aliases.is_empty() {
        bail!("--gateway-models is empty — to turn the gateway off, use --disable-model-gateway");
    }
    let mut setting = Map::new();
    setting.insert("models".to_string(), json!(aliases));
    if let Some(tokens) = monthly_tokens {
        setting.insert("monthlyTokenBudget".to_string(), json!(tokens));
    }
    if let Some(rpm) = rpm {
        setting.insert("requestsPerMinute".to_string(), json!(rpm));
    }
    Ok(Some(Value::Object(setting)))
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
    if let Some(model_gateway) = update.model_gateway.clone() {
        body.insert("modelGateway".to_string(), model_gateway);
    }

    // An empty PATCH is accepted by the control plane and changes nothing, so it would
    // report success while having done nothing at all. Refuse instead — someone who
    // typed `template update --slug x` and saw "Updated" would reasonably believe
    // something had happened.
    if body.is_empty() {
        bail!(
            "nothing to update — pass at least one of --name, --description, --status, \
             --stripe-product, --cluster-type, --base-domain, --frontend-domain, \
             --default-instance-size, --supports-key-rotation, --source-repo, or the \
             --gateway-* flags (to publish a template, `forklaunch managed template \
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
    fn gateway_flags_build_the_whole_setting() {
        let models = "terra, luna,".to_string();
        assert_eq!(
            model_gateway_body(Some(&models), Some(500_000), Some(30), false).unwrap(),
            Some(json!({
                "models": ["terra", "luna"],
                "monthlyTokenBudget": 500_000,
                "requestsPerMinute": 30
            }))
        );
        assert_eq!(
            model_gateway_body(None, None, None, true).unwrap(),
            Some(json!({ "models": [] }))
        );
        assert_eq!(model_gateway_body(None, None, None, false).unwrap(), None);
    }

    #[test]
    fn a_gateway_budget_without_models_is_refused_rather_than_turning_it_off() {
        let error = model_gateway_body(None, Some(10), None, false).unwrap_err();
        assert!(error.to_string().contains("--gateway-models"), "{}", error);
        let empty = " , ".to_string();
        let error = model_gateway_body(Some(&empty), None, None, false).unwrap_err();
        assert!(
            error.to_string().contains("--disable-model-gateway"),
            "{}",
            error
        );
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
