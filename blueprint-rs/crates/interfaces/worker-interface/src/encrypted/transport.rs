//! Connection admission is separate from encrypted event payloads. These checks
//! require the broker clients to retain certificate and hostname verification.
use crate::encrypted::ProcessorError;
use std::{collections::BTreeMap, net::IpAddr};
use url::{Host, Url};

#[derive(Clone, Copy, Debug, Default)]
pub enum BrokerTransportPolicy {
    #[default]
    TlsOrLoopback,
    /// Explicitly opt into plaintext on a trusted local development network.
    PlaintextDevelopment,
}

impl BrokerTransportPolicy {
    /// # Errors
    /// Rejects malformed URLs, unsupported schemes, verification bypasses,
    /// and remote plaintext unless the development policy explicitly allows it.
    pub fn redis(self, endpoint: &str) -> Result<(), ProcessorError> {
        let url = parse(endpoint)?;
        // Redis supports #insecure to disable certificate verification. Never
        // admit it as verified TLS, even under the development exception.
        if url.fragment().is_some() || !matches!(url.scheme(), "redis" | "rediss") {
            return Err(ProcessorError::Configuration);
        }
        self.admit(url.scheme() == "rediss", loopback(&url))
    }

    /// # Errors
    /// Rejects malformed URLs, ambiguous routing/TLS options, and remote
    /// connections without verify-full unless development plaintext is allowed.
    pub fn postgres(self, endpoint: &str) -> Result<(), ProcessorError> {
        let url = parse(endpoint)?;
        if !matches!(url.scheme(), "postgres" | "postgresql") || url.fragment().is_some() {
            return Err(ProcessorError::Configuration);
        }
        // Reject duplicate/alternate routing options rather than allowing the
        // parser and driver to disagree about the actual destination or mode.
        let mut mode = None;
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "sslmode" | "ssl-mode" => {
                    if mode.replace(value.into_owned()).is_some() {
                        return Err(ProcessorError::Configuration);
                    }
                }
                "host" | "hostaddr" | "port" => return Err(ProcessorError::Configuration),
                _ => {}
            }
        }
        self.admit(mode.as_deref() == Some("verify-full"), loopback(&url))
    }

    /// # Errors
    /// Rejects malformed broker addresses, unknown security protocols, TLS
    /// verification bypasses, and remote plaintext outside development policy.
    pub fn kafka(
        self,
        brokers: &str,
        properties: &BTreeMap<String, String>,
    ) -> Result<(), ProcessorError> {
        let endpoints = brokers.split(',').map(str::trim).collect::<Vec<_>>();
        if endpoints.is_empty() || endpoints.iter().any(|host| host.is_empty()) {
            return Err(ProcessorError::Configuration);
        }
        let mut local = true;
        for endpoint in endpoints {
            let url = parse(&format!("kafka://{endpoint}"))?;
            if !url.username().is_empty()
                || url.password().is_some()
                || url.path() != ""
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(ProcessorError::Configuration);
            }
            local &= loopback(&url);
        }
        let protocol = properties
            .get("security.protocol")
            .map_or("PLAINTEXT", String::as_str);
        if !matches!(
            protocol,
            "SSL" | "SASL_SSL" | "PLAINTEXT" | "SASL_PLAINTEXT"
        ) {
            return Err(ProcessorError::Configuration);
        }
        if properties
            .get("enable.ssl.certificate.verification")
            .is_some_and(|v| v != "true")
            || properties
                .get("ssl.endpoint.identification.algorithm")
                .is_some_and(|v| !v.eq_ignore_ascii_case("https"))
        {
            return Err(ProcessorError::Configuration);
        }
        self.admit(matches!(protocol, "SSL" | "SASL_SSL"), local)
    }

    fn admit(self, tls: bool, local: bool) -> Result<(), ProcessorError> {
        if tls || local || matches!(self, Self::PlaintextDevelopment) {
            Ok(())
        } else {
            Err(ProcessorError::Configuration)
        }
    }
}

fn parse(endpoint: &str) -> Result<Url, ProcessorError> {
    let url = Url::parse(endpoint).map_err(|_| ProcessorError::Configuration)?;
    if url.host().is_none() {
        return Err(ProcessorError::Configuration);
    }
    Ok(url)
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => {
            name == "localhost" || name.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        }
        Some(Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_requires_verified_tls_and_development_is_explicit() {
        let policy = BrokerTransportPolicy::default();
        assert!(policy.redis("redis://localhost:6379").is_ok());
        assert!(policy.redis("redis://127.0.0.1:6379").is_ok());
        assert!(policy.postgres("postgres://127.0.0.1/db").is_ok());
        assert!(policy.kafka("127.0.0.1:9092", &BTreeMap::new()).is_ok());
        assert!(policy.redis("redis://[::1]:6379").is_ok());
        assert!(
            policy
                .redis("redis://user:secret@remote.example:6379")
                .is_err()
        );
        assert!(policy.redis("rediss://remote.example:6379").is_ok());
        assert!(
            policy
                .redis("rediss://remote.example:6379/#insecure")
                .is_err()
        );
        assert!(policy.redis("redis://localhost.evil.example:6379").is_err());
        assert!(
            BrokerTransportPolicy::PlaintextDevelopment
                .redis("redis://redis:6379")
                .is_ok()
        );
        assert!(
            policy
                .postgres("postgres://remote/db?sslmode=require")
                .is_err()
        );
        assert!(
            policy
                .postgres("postgres://remote/db?sslmode=verify-full")
                .is_ok()
        );
        assert!(
            policy
                .postgres("postgres://remote/db?sslmode=verify-full&sslmode=disable")
                .is_err()
        );
        assert!(
            policy
                .postgres("postgres://localhost/db?host=remote")
                .is_err()
        );
    }
    #[test]
    fn kafka_rejects_plaintext_remote_and_verification_bypasses() {
        let policy = BrokerTransportPolicy::default();
        let mut properties = BTreeMap::new();
        assert!(
            policy
                .kafka("localhost:9092,[::1]:9092", &properties)
                .is_ok()
        );
        assert!(
            policy
                .kafka("localhost:9092,remote:9092", &properties)
                .is_err()
        );
        properties.insert("security.protocol".into(), "SASL_SSL".into());
        assert!(policy.kafka("remote:9092", &properties).is_ok());
        properties.insert("enable.ssl.certificate.verification".into(), "false".into());
        assert!(policy.kafka("remote:9092", &properties).is_err());
        properties.remove("enable.ssl.certificate.verification");
        properties.insert(
            "ssl.endpoint.identification.algorithm".into(),
            "none".into(),
        );
        assert!(policy.kafka("remote:9092", &properties).is_err());
        assert!(policy.kafka("", &properties).is_err());
    }
}
