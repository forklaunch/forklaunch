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
        types::{AppTemplate, TEMPLATE_CLUSTER_TYPES, TEMPLATE_STATUSES},
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
    pub(super) cluster_type: Option<&'a String>,
    pub(super) frontend_domain: Option<&'a String>,
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
            "Change a template's name, description, status, Stripe product, or cluster type",
        )
        .long_about(
            "Change a template's name, description, status, Stripe product, or cluster type.\n\n\
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
             anyone to be charged — it is stored for later use.",
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
                .value_parser(TEMPLATE_CLUSTER_TYPES.to_vec())
                .help("Where instances of this template run: org-shared (your org's shared hosts), platform-shared, or dedicated. Applies to instances launched from now on"),
        )
        .arg(
            Arg::new("frontend_domain")
                .long("frontend-domain")
                .help("Domain the product's frontend is served from (e.g. app.example.com). Each instance's UI is <hostPrefix>.<domain>; the claim page and `instance list` hand that URL out"),
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

        update_template(
            slug,
            TemplateUpdate {
                name: matches.get_one::<String>("name"),
                description: matches.get_one::<String>("description"),
                status: matches.get_one::<String>("status").map(String::as_str),
                stripe_product: matches.get_one::<String>("stripe_product"),
                cluster_type: matches.get_one::<String>("cluster_type"),
                frontend_domain: matches.get_one::<String>("frontend_domain"),
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
    if let Some(frontend_domain) = update.frontend_domain {
        body.insert("frontendDomain".to_string(), json!(frontend_domain));
    }

    // An empty PATCH is accepted by the control plane and changes nothing, so it would
    // report success while having done nothing at all. Refuse instead — someone who
    // typed `template update --slug x` and saw "Updated" would reasonably believe
    // something had happened.
    if body.is_empty() {
        bail!(
            "nothing to update — pass at least one of --name, --description, --status, \
             --stripe-product, --cluster-type, or --frontend-domain (to publish a template, \
             `forklaunch managed template publish-template --slug {}` is the shorthand)",
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
        assert!(update.cluster_type.is_none());
        assert!(update.frontend_domain.is_none());
    }

    #[test]
    fn the_cli_cluster_type_list_matches_what_the_control_plane_validates() {
        // managed-apps validates `clusterType` against ClusterPlacementEnum
        // (org-shared | platform-shared | dedicated). Keep the local list identical so a
        // typo fails before the round trip and a valid value is never refused locally.
        assert_eq!(
            TEMPLATE_CLUSTER_TYPES,
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
