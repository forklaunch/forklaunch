use serde::{Deserialize, Serialize};

/// One managed instance as the control plane reports it.
///
/// Every field past `id` is optional on purpose. The `/managed-mode` routes are being
/// written in parallel with this CLI, and the summary endpoint and the (not yet
/// implemented) list endpoint return overlapping but not identical projections. A
/// missing field should render as a blank column, not fail the whole command.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManagedInstance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) template_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) relay_eligible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) current_version_semver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) claimed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) created_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AppTemplate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) source_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) cluster_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) frontend_domain: Option<String>,
}

/// Where a template's instances run. Same vocabulary as `forklaunch app hosting`
/// and `deploy create --cluster-type`; the control plane validates it too, but a
/// typo should fail before the round trip.
///
///   org-shared       your organization's shared hosts (the managed default)
///   platform-shared  ForkLaunch's shared hosts (cheapest, cross-tenant compute)
///   dedicated        a cluster of the instance's own
pub(super) const TEMPLATE_CLUSTER_TYPES: &[&str] = &["org-shared", "platform-shared", "dedicated"];

/// The template statuses the platform defines. A template is created as `draft`;
/// `instance create` requires `published`, so a template stays uninstantiable until
/// something moves it. The control plane validates this list server-side and answers
/// 400 with the allowed values, but validating here too turns a typo into an instant
/// local error instead of a round trip.
pub(super) const TEMPLATE_STATUSES: &[&str] = &["draft", "published", "retired"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TemplateVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) semver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) published_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ClaimLink {
    pub(super) claim_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) expires_at: Option<String>,
}

/// The instance lifecycle states the platform defines, in rough lifecycle order.
///
/// Used to validate and document `--state` in `--help`. Kept as a plain list rather
/// than an enum because the CLI only ever passes these through to the control plane
/// and displays them back — it never branches on them.
pub(super) const INSTANCE_STATES: &[&str] = &[
    "provisioning",
    "provisioning_failed",
    "awaiting_claim",
    "awaiting_claim_blocked",
    "active",
    "suspended",
    "destroying",
    "destroyed",
];

/// Where a template variable's value comes from.
///
/// The three kinds exist because the answer to "where does this value come from" is
/// genuinely different, not because someone wanted three flavors of the same thing:
///
/// - `static`    the same literal for every instance; the TEMPLATE holds the value
/// - `generated` a recipe, not a value; each INSTANCE derives its own
/// - `custom`    the maintainer types it in per instance; the INSTANCE holds the value
pub(super) const VARIABLE_KINDS: &[&str] = &["static", "generated", "custom"];

/// How far a variable reaches in the deployed app. Mirrors the platform's own
/// environment-variable scoping: `application` reaches every service, `service` reaches
/// exactly one named service.
pub(super) const VARIABLE_SCOPES: &[&str] = &["application", "service"];

/// The generator recipes a `generated` variable may name.
///
/// This is platform-management's `generateKeyMaterial` vocabulary, not a list this CLI
/// invented — the same strings the platform's own `component_property` column is
/// constrained to. Validating them here turns a typo into an instant local error rather
/// than a template that provisions every instance with an empty variable.
pub(super) const GENERATOR_TYPES: &[&str] = &[
    "32-bytes-base64",
    "64-bytes-base64",
    "hex-key",
    "key-material",
    "private-pem",
    "public-pem",
];

/// One variable a template declares, as the control plane reports it.
///
/// Mirrors the control plane's `TemplateVariableSchema`, where `key`, `scope`, `kind`
/// and `required` are non-optional and the rest are not. Everything is `Option` here
/// anyway, for the same reason `ManagedInstance`'s fields are: a field a future server
/// stops sending should render as a blank column rather than failing the whole command.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TemplateVariable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) service_name: Option<String>,
    // There is deliberately NO `value` field. The control plane's
    // `TemplateVariableSchema` omits it on both the managed-apps handler and the
    // `/managed-mode` proxy: a `static` value can be a credential shared by every
    // instance, and a list endpoint is the wrong place to hand one back. Adding the
    // field here would only ever deserialize to `None` and invite a column that is
    // always blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) generator_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
}

/// One declared variable as it applies to ONE instance.
///
/// This is the template's declaration plus, for `custom` variables, whether that
/// instance has a value yet. It is deliberately not the value itself.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstanceVariable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) service_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    /// Whether this instance has a value for a `custom` variable. The control plane's
    /// `InstanceVariableStatusSchema` calls it `isSet`; the other two aliases are kept
    /// because they were the plausible alternatives while this was being written, and
    /// reading only one name would render every variable MISSING if it ever changed.
    #[serde(
        default,
        alias = "isSet",
        alias = "set",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) has_value: Option<bool>,
    /// NEVER printed and NEVER re-serialized, including under `--json`.
    ///
    /// The field exists only so that a control plane which returns the value anyway
    /// still yields a correct SET/MISSING answer — dropping it at parse time would make
    /// this CLI report MISSING for a variable that is set. `skip_serializing` is what
    /// keeps it from leaking back out of `--json`, and `set_state` is the only thing
    /// allowed to look at it.
    #[serde(default, skip_serializing)]
    pub(super) value: Option<String>,
}

/// Whether an instance has a value for a `custom` variable.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SetState {
    Set,
    Missing,
    /// The control plane said nothing either way. Reported as `?` rather than guessed:
    /// claiming MISSING would send someone hunting for a value that is already there,
    /// and claiming SET would hide a variable that will fail the provision.
    Unknown,
}

impl InstanceVariable {
    /// Reads set-ness from whichever signal the control plane provided, preferring the
    /// explicit boolean over inferring it from a value that should not have been sent.
    pub(super) fn set_state(&self) -> SetState {
        let known = self
            .has_value
            .or_else(|| self.value.as_ref().map(|value| !value.is_empty()));
        match known {
            Some(true) => SetState::Set,
            Some(false) => SetState::Missing,
            None => SetState::Unknown,
        }
    }

    /// True when this variable will block `instance create`: a required `custom`
    /// variable with no value.
    pub(super) fn blocks_provisioning(&self) -> bool {
        self.kind.as_deref() == Some("custom")
            && self.required == Some(true)
            && self.set_state() == SetState::Missing
    }
}

pub(super) fn dash(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}

/// Renders an optional boolean for a table cell, where "the server did not say" and
/// "the server said false" are different facts.
fn yes_no(value: &Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "-",
    }
}

/// Renders the REQUIRED column.
///
/// `required` only means anything for `custom` — the CLI refuses `--required` on the
/// other two kinds, because a static variable always has a value and a generated one is
/// always derivable, so neither can be missing at launch. The control plane nonetheless
/// sends `required: false` on every row, since its schema declares the field
/// non-optional. Printing "no" against a static variable would imply the flag means
/// something there, so those rows get a dash instead.
pub(super) fn required_cell(kind: &Option<String>, required: &Option<bool>) -> &'static str {
    match kind.as_deref() {
        Some("custom") => yes_no(required),
        _ => "-",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_reads_as_a_dash_for_the_kinds_it_cannot_apply_to() {
        // The control plane sends `required: false` on every row because its schema
        // declares the field non-optional; only `custom` rows should render it.
        for kind in ["static", "generated"] {
            assert_eq!(
                required_cell(&Some(kind.to_string()), &Some(false)),
                "-",
                "{}",
                kind
            );
        }
        assert_eq!(
            required_cell(&Some("custom".to_string()), &Some(true)),
            "yes"
        );
        assert_eq!(
            required_cell(&Some("custom".to_string()), &Some(false)),
            "no"
        );
        // A kind the CLI has not been told about should not claim to know either.
        assert_eq!(required_cell(&None, &Some(true)), "-");
    }
}
