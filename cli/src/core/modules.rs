use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::constants::{Module, get_service_module_name};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IamConfig {
    BetterAuthIam,
    BaseIam,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BillingConfig {
    BaseBilling,
    StripeBilling,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EcommerceConfig {
    StripeEcommerce,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MessagingConfig {
    BaseMessaging,
    TwilioMessaging,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CacConfig {
    BaseCac,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RelayConfig {
    Relay,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub(crate) struct ModuleConfig {
    pub(crate) iam: Option<IamConfig>,
    pub(crate) billing: Option<BillingConfig>,
    pub(crate) ecommerce: Option<EcommerceConfig>,
    pub(crate) messaging: Option<MessagingConfig>,
    pub(crate) cac: Option<CacConfig>,
    pub(crate) relay: Option<RelayConfig>,
}

pub(crate) fn validate_modules(
    modules: &Vec<Module>,
    global_module_config: &mut ModuleConfig,
) -> Result<()> {
    let mut module_conflict_set = HashSet::new();

    for module in modules {
        match module {
            Module::BetterAuthIam => {
                global_module_config.iam = Some(IamConfig::BetterAuthIam);
            }
            Module::BaseIam => {
                global_module_config.iam = Some(IamConfig::BaseIam);
            }
            Module::BaseBilling => {
                global_module_config.billing = Some(BillingConfig::BaseBilling);
            }
            Module::StripeBilling => {
                global_module_config.billing = Some(BillingConfig::StripeBilling);
            }
            Module::StripeEcommerce => {
                global_module_config.ecommerce = Some(EcommerceConfig::StripeEcommerce);
            }
            Module::BaseMessaging => {
                global_module_config.messaging = Some(MessagingConfig::BaseMessaging);
            }
            Module::TwilioMessaging => {
                global_module_config.messaging = Some(MessagingConfig::TwilioMessaging);
            }
            Module::BaseCac => {
                global_module_config.cac = Some(CacConfig::BaseCac);
            }
            Module::Relay => {
                global_module_config.relay = Some(RelayConfig::Relay);
            }
        }

        let module_type = get_service_module_name(module);

        if module_conflict_set.contains(&module_type) {
            bail!("Module conflict");
        }

        module_conflict_set.insert(module_type);
    }

    Ok(())
}
