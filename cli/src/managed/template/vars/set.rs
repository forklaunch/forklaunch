use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            Missing, print_dryrun, put_json_optional, require_managed_mode, resolve_managed_auth,
        },
        template::vars::resolve_scope,
        types::{GENERATOR_TYPES, VARIABLE_KINDS, VARIABLE_SCOPES},
    },
};

/// Everything `PUT /managed-mode/templates/:slug/variables` needs, before validation.
///
/// Taken as a struct rather than an `ArgMatches` so the flag-combination rules can be
/// unit tested without standing up a control plane — they are the part most likely to
/// be got wrong, and they are pure.
#[derive(Debug, Default)]
pub(super) struct VariableSpec<'a> {
    pub(super) key: &'a str,
    pub(super) kind: &'a str,
    pub(super) scope: &'a str,
    pub(super) service: Option<&'a str>,
    pub(super) value: Option<&'a str>,
    pub(super) generator: Option<&'a str>,
    pub(super) required: bool,
    pub(super) description: Option<&'a str>,
}

/// Validates the flag combination and builds the request body.
///
/// The rules enforced here are all ones where the client-side error is meaningfully
/// clearer than the server's. The server would reject `--value` on a `generated`
/// variable too, but as a schema error naming a field the operator never typed; here it
/// can say what the operator should have used instead.
pub(super) fn build_variable_body(spec: &VariableSpec<'_>) -> Result<Value> {
    if spec.key.trim().is_empty() {
        bail!("--key cannot be empty");
    }
    // A key containing `=` is nearly always a paste error — someone typed
    // `--key FOO=bar` meaning `--key FOO --value bar`. Accepting it would create a
    // variable literally named `FOO=bar`, which the deployed app can never read.
    if spec.key.contains('=') {
        bail!(
            "--key '{}' looks like a KEY=VALUE pair — pass the name and the value separately: \
             --key {} --value {}",
            spec.key,
            spec.key
                .split_once('=')
                .map(|pair| pair.0)
                .unwrap_or(spec.key),
            spec.key
                .split_once('=')
                .map(|pair| pair.1)
                .unwrap_or("<value>")
        );
    }
    if spec.key.chars().any(char::is_whitespace) {
        bail!(
            "--key '{}' contains whitespace — an environment variable name cannot",
            spec.key
        );
    }

    let service = resolve_scope(spec.scope, spec.service)?;

    match spec.kind {
        "static" => {
            if spec.value.is_none() {
                bail!(
                    "--value is required with --kind static — a static variable IS its literal \
                     value, the same one for every instance. If you want each instance to get \
                     its own, use --kind generated; if you want to fill it in per instance, use \
                     --kind custom."
                );
            }
            if spec.generator.is_some() {
                bail!(
                    "--generator is meaningless with --kind static — a static variable holds a \
                     literal, not a recipe. Use --kind generated to have each instance derive \
                     its own value."
                );
            }
            if spec.required {
                bail!(
                    "--required only applies to --kind custom — a static variable always has a \
                     value, so it can never be missing at launch."
                );
            }
        }
        "generated" => {
            if spec.generator.is_none() {
                bail!(
                    "--generator is required with --kind generated — it names the recipe each \
                     instance derives its value from. One of: {}",
                    GENERATOR_TYPES.join(", ")
                );
            }
            if spec.value.is_some() {
                bail!(
                    "--value is meaningless with --kind generated — the point of a generated \
                     variable is that the template stores NO value; each instance derives its \
                     own, seeded on its instance id. Use --kind static to set one literal for \
                     every instance instead."
                );
            }
            if spec.required {
                bail!(
                    "--required only applies to --kind custom — a generated variable is always \
                     derivable, so it can never be missing at launch."
                );
            }
        }
        "custom" => {
            if spec.value.is_some() {
                bail!(
                    "--value is meaningless with --kind custom — the template only declares that \
                     the variable exists. Set the value per instance with: forklaunch managed \
                     instance vars set --id <instance-id> --key {} --value <value>",
                    spec.key
                );
            }
            if spec.generator.is_some() {
                bail!(
                    "--generator is meaningless with --kind custom — a custom value is typed in \
                     per instance, not derived. Use --kind generated to derive one instead."
                );
            }
        }
        other => bail!(
            "unknown --kind '{}' — expected one of: {}",
            other,
            VARIABLE_KINDS.join(", ")
        ),
    }

    let mut body = Map::new();
    body.insert("key".to_string(), json!(spec.key));
    body.insert("scope".to_string(), json!(spec.scope));
    body.insert("kind".to_string(), json!(spec.kind));
    if let Some(service) = service {
        body.insert("serviceName".to_string(), json!(service));
    }
    if let Some(value) = spec.value {
        body.insert("value".to_string(), json!(value));
    }
    if let Some(generator) = spec.generator {
        body.insert("generatorType".to_string(), json!(generator));
    }
    // Sent explicitly rather than only when true. This is an upsert, so re-running
    // `set` on an existing required variable without `--required` has to be able to
    // clear the flag — omitting the field would leave the old value in place and make
    // the flag one-way.
    if spec.kind == "custom" {
        body.insert("required".to_string(), json!(spec.required));
    }
    if let Some(description) = spec.description {
        body.insert("description".to_string(), json!(description));
    }

    Ok(Value::Object(body))
}

#[derive(Debug)]
pub(super) struct SetCommand;

impl SetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command(
            "set",
            "Declare (or redeclare) a variable every instance of a template receives",
        )
        .long_about(
            "Declare (or redeclare) a variable every instance of a template receives.\n\n\
             This is an upsert: running it again for the same key, scope and service replaces\n\
             the declaration rather than adding a second one.\n\n\
             THE KIND DECIDES WHICH OTHER FLAGS APPLY:\n\n\
             \x20 --kind static     requires --value. Every instance gets that same literal.\n\
             \x20 --kind generated  requires --generator. Each instance derives its own value\n\
             \x20                   from that recipe, seeded on its instance id so a\n\
             \x20                   provisioning retry produces the SAME value rather than a\n\
             \x20                   new one that no longer matches what was already deployed.\n\
             \x20                   The template stores no secret.\n\
             \x20 --kind custom     takes neither. The template just declares that the\n\
             \x20                   variable exists; you set each instance's value with\n\
             \x20                   `managed instance vars set`. Add --required to make an\n\
             \x20                   instance refuse to provision until it has one.\n\n\
             Passing a flag that does not apply to the chosen kind is an error, not a\n\
             warning — `--value` with `--kind generated` almost always means the wrong kind\n\
             was chosen, and silently dropping the value would publish a template that hands\n\
             every customer a secret nobody meant to share.\n\n\
             A `static` value cannot be read back afterwards — `vars list` reports that one is\n\
             set, not what it is. Setting it again is how you change it, and is the only way\n\
             to correct a typo in one.\n\n\
             Examples:\n\
             \x20 vars set --slug clinic --key LOG_LEVEL --kind static --value info\n\
             \x20 vars set --slug clinic --key SESSION_SECRET --kind generated --generator 32-bytes-base64\n\
             \x20 vars set --slug clinic --key STRIPE_KEY --kind custom --required --scope service --service billing",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template to declare the variable on"),
        )
        .arg(
            Arg::new("key")
                .long("key")
                .required(true)
                .help("Environment variable name the deployed app will read"),
        )
        .arg(
            Arg::new("kind")
                .long("kind")
                .required(true)
                .value_parser(VARIABLE_KINDS.to_vec())
                .help("Where the value comes from: static | generated | custom"),
        )
        .arg(
            Arg::new("value")
                .long("value")
                .help("The literal value — `--kind static` only"),
        )
        .arg(
            Arg::new("generator")
                .long("generator")
                .value_parser(GENERATOR_TYPES.to_vec())
                .help("Recipe each instance derives its value from — `--kind generated` only"),
        )
        .arg(
            Arg::new("required")
                .long("required")
                .help("Refuse to provision an instance until it has a value — `--kind custom` only")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("scope")
                .long("scope")
                .value_parser(VARIABLE_SCOPES.to_vec())
                .default_value("application")
                .help("How far the variable reaches: application (every service) or service (one)"),
        )
        .arg(
            Arg::new("service")
                .long("service")
                .help("Which service the variable reaches — required with `--scope service`"),
        )
        .arg(
            Arg::new("description")
                .long("description")
                .help("Shown to whoever fills in a custom value"),
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
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let key = matches
            .get_one::<String>("key")
            .context("--key is required")?;
        let kind = matches
            .get_one::<String>("kind")
            .context("--kind is required")?;
        let scope = matches
            .get_one::<String>("scope")
            .map(String::as_str)
            .unwrap_or("application");

        let body = build_variable_body(&VariableSpec {
            key,
            kind,
            scope,
            service: matches.get_one::<String>("service").map(String::as_str),
            value: matches.get_one::<String>("value").map(String::as_str),
            generator: matches.get_one::<String>("generator").map(String::as_str),
            required: matches.get_flag("required"),
            description: matches.get_one::<String>("description").map(String::as_str),
        })?;

        // Slugs are organization-authored identifiers, but they still land in a URL
        // path — encode so a slug containing a separator cannot restructure the request.
        let path = format!("/templates/{}/variables", urlencoding::encode(slug));

        if matches.get_flag("dryrun") {
            return print_dryrun("PUT", &path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let response = put_json_optional(
            &path,
            body,
            Missing::Resource(format!("template '{}'", slug)),
        )?;

        if matches.get_flag("json") {
            // An upsert that answered 204 has nothing to print; say so in JSON rather
            // than printing nothing at all and leaving a script unable to tell success
            // from a swallowed error.
            let payload = response.unwrap_or_else(|| json!({ "status": "ok" }));
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }

        let where_it_reaches = match matches.get_one::<String>("service") {
            Some(service) => format!("service '{}'", service),
            None => "every service".to_string(),
        };
        log_ok!(
            stdout,
            "Declared {} variable '{}' on template '{}' — reaches {}",
            kind,
            key,
            slug,
            where_it_reaches
        );

        match kind.as_str() {
            "custom" if matches.get_flag("required") => log_info!(
                stdout,
                "Instances now REFUSE to provision until this has a value: forklaunch managed instance vars set --id <instance-id> --key {} --value <value>",
                key
            ),
            "custom" => log_info!(
                stdout,
                "Set each instance's value with: forklaunch managed instance vars set --id <instance-id> --key {} --value <value>",
                key
            ),
            "generated" => log_info!(
                stdout,
                "Each instance derives its own value; the template stores no secret. Existing instances pick it up on their next provision."
            ),
            _ => log_info!(
                stdout,
                "Existing instances pick this up on their next provision."
            ),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>(kind: &'a str) -> VariableSpec<'a> {
        VariableSpec {
            key: "SESSION_SECRET",
            kind,
            scope: "application",
            ..Default::default()
        }
    }

    #[test]
    fn static_requires_a_value() {
        let error = build_variable_body(&spec("static"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("--value is required"), "{}", error);
    }

    #[test]
    fn static_refuses_a_generator() {
        let error = build_variable_body(&VariableSpec {
            value: Some("info"),
            generator: Some("hex-key"),
            ..spec("static")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("--generator is meaningless"), "{}", error);
    }

    #[test]
    fn generated_refuses_a_value() {
        // The important one. Silently dropping the value would publish a template that
        // looks like it pins a secret and does not; accepting it would store one.
        let error = build_variable_body(&VariableSpec {
            value: Some("hunter2"),
            generator: Some("32-bytes-base64"),
            ..spec("generated")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("--value is meaningless"), "{}", error);
        assert!(error.contains("--kind static"), "{}", error);
    }

    #[test]
    fn generated_requires_a_generator() {
        let error = build_variable_body(&spec("generated"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("--generator is required"), "{}", error);
        // The message has to list the recipes; there is nowhere else to discover them.
        assert!(error.contains("32-bytes-base64"), "{}", error);
        assert!(error.contains("private-pem"), "{}", error);
    }

    #[test]
    fn custom_refuses_both_a_value_and_a_generator() {
        let value_error = build_variable_body(&VariableSpec {
            value: Some("sk_live_x"),
            ..spec("custom")
        })
        .unwrap_err()
        .to_string();
        assert!(value_error.contains("instance vars set"), "{}", value_error);

        let generator_error = build_variable_body(&VariableSpec {
            generator: Some("hex-key"),
            ..spec("custom")
        })
        .unwrap_err()
        .to_string();
        assert!(
            generator_error.contains("--generator is meaningless"),
            "{}",
            generator_error
        );
    }

    #[test]
    fn required_only_applies_to_custom() {
        for (kind, extra) in [("static", Some("info")), ("generated", None)] {
            let error = build_variable_body(&VariableSpec {
                required: true,
                value: extra,
                generator: if kind == "generated" {
                    Some("hex-key")
                } else {
                    None
                },
                ..spec(kind)
            })
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("--required only applies to --kind custom"),
                "{}: {}",
                kind,
                error
            );
        }
    }

    #[test]
    fn service_scope_needs_a_service_name() {
        let error = build_variable_body(&VariableSpec {
            scope: "service",
            ..spec("custom")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("--service is required"), "{}", error);
    }

    #[test]
    fn a_key_that_is_really_a_pair_is_refused() {
        let error = build_variable_body(&VariableSpec {
            key: "LOG_LEVEL=info",
            value: Some("info"),
            ..spec("static")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("KEY=VALUE"), "{}", error);
        assert!(error.contains("--key LOG_LEVEL --value info"), "{}", error);
    }

    #[test]
    fn a_valid_static_declaration_builds_the_expected_body() {
        let body = build_variable_body(&VariableSpec {
            key: "LOG_LEVEL",
            value: Some("info"),
            description: Some("verbosity"),
            ..spec("static")
        })
        .unwrap();
        assert_eq!(body["key"], json!("LOG_LEVEL"));
        assert_eq!(body["kind"], json!("static"));
        assert_eq!(body["scope"], json!("application"));
        assert_eq!(body["value"], json!("info"));
        assert_eq!(body["description"], json!("verbosity"));
        assert!(body.get("serviceName").is_none());
        assert!(body.get("generatorType").is_none());
        // `required` is a custom-only concept; sending it for a static variable would
        // ask the control plane to store a flag that can never be true.
        assert!(body.get("required").is_none());
    }

    #[test]
    fn a_valid_generated_declaration_carries_the_recipe_and_no_value() {
        let body = build_variable_body(&VariableSpec {
            generator: Some("32-bytes-base64"),
            ..spec("generated")
        })
        .unwrap();
        assert_eq!(body["generatorType"], json!("32-bytes-base64"));
        assert!(
            body.get("value").is_none(),
            "a generated declaration must never carry a value: {}",
            body
        );
    }

    #[test]
    fn a_custom_declaration_always_states_required_either_way() {
        // `set` is an upsert, so an omitted `required` would make the flag one-way:
        // once set, nothing could ever clear it.
        let required = build_variable_body(&VariableSpec {
            required: true,
            scope: "service",
            service: Some("billing"),
            ..spec("custom")
        })
        .unwrap();
        assert_eq!(required["required"], json!(true));
        assert_eq!(required["serviceName"], json!("billing"));
        assert_eq!(required["scope"], json!("service"));

        let optional = build_variable_body(&spec("custom")).unwrap();
        assert_eq!(optional["required"], json!(false));
    }

    #[test]
    fn the_generator_list_matches_the_platforms_key_material_vocabulary() {
        // These are platform-management's `generateKeyMaterial` names, not names this
        // CLI chose. If they drift, every generated variable resolves to nothing.
        assert_eq!(
            GENERATOR_TYPES,
            &[
                "32-bytes-base64",
                "64-bytes-base64",
                "hex-key",
                "key-material",
                "private-pem",
                "public-pem",
            ]
        );
    }
}
