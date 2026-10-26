//! Host port preflight for `docker compose up`.
//!
//! Generated applications publish fixed default host ports (services from 8000,
//! Postgres 5432, Redis 6379, ...), numbered per application. Two applications
//! on one machine therefore collide. Depending on the Docker engine, a failed
//! bind either aborts `up` with an opaque error or leaves the container running
//! with no published port, which looks healthy while the host cannot reach it.
//! [`preflight`] checks every port the selected services publish before
//! `up` and fails with the port, the service and, where Docker knows it, the
//! container holding it.
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket},
    process::{Command, Stdio},
};

use anyhow::{Result, bail};
use serde_json::Value;

/// One host port a Compose service publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedPort {
    pub(crate) service: String,
    pub(crate) host_ip: Option<String>,
    pub(crate) port: u16,
    pub(crate) protocol: String,
}

/// Who holds a port, as far as the preflight can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Holder {
    /// Free on the host.
    Free,
    /// Published by a container of this same Compose project (a re-run).
    ThisProject,
    /// Taken by something else; the description names it when known.
    Other(Option<String>),
}

/// The host ports `services` publish in a resolved Compose model
/// (`docker compose config --format json`). An empty `services` means all.
/// Long-form (`{published, host_ip, protocol}`) and short-form
/// (`"127.0.0.1:6379:6379/tcp"`) entries are both read; ranges contribute
/// every port, and entries without a published port are skipped.
pub(crate) fn published_ports(compose: &Value, services: &[String]) -> Vec<PublishedPort> {
    let mut ports = Vec::new();
    let Some(all) = compose.get("services").and_then(Value::as_object) else {
        return ports;
    };
    for (name, service) in all {
        if !services.is_empty() && !services.iter().any(|selected| selected == name) {
            continue;
        }
        for entry in service.get("ports").and_then(Value::as_array).into_iter().flatten() {
            let (host_ip, published, protocol) = match entry {
                Value::Object(entry) => (
                    entry.get("host_ip").and_then(Value::as_str).map(str::to_owned),
                    entry.get("published").and_then(|value| match value {
                        Value::String(value) => Some(value.clone()),
                        Value::Number(value) => Some(value.to_string()),
                        _ => None,
                    }),
                    entry.get("protocol").and_then(Value::as_str).unwrap_or("tcp").to_owned(),
                ),
                Value::String(entry) => match parse_short(entry) {
                    Some(parsed) => parsed,
                    None => continue,
                },
                Value::Number(_) => continue, // container port only
                _ => continue,
            };
            let Some(published) = published.filter(|value| !value.is_empty()) else {
                continue;
            };
            let (start, end) = match published.split_once('-') {
                Some((start, end)) => (start.parse::<u16>(), end.parse::<u16>()),
                None => (published.parse::<u16>(), published.parse::<u16>()),
            };
            let (Ok(start), Ok(end)) = (start, end) else { continue };
            for port in start..=end.max(start) {
                if port == 0 {
                    continue;
                }
                ports.push(PublishedPort {
                    service: name.clone(),
                    host_ip: host_ip.clone().filter(|ip| !ip.is_empty()),
                    port,
                    protocol: protocol.clone(),
                });
            }
        }
    }
    ports
}

/// `[host_ip:][published:]target[/protocol]`; only entries with a published
/// port are returned.
fn parse_short(entry: &str) -> Option<(Option<String>, Option<String>, String)> {
    let (mapping, protocol) = match entry.rsplit_once('/') {
        Some((mapping, protocol)) => (mapping, protocol.to_owned()),
        None => (entry, "tcp".to_owned()),
    };
    // IPv6 host addresses are bracketed: [::1]:8000:8000.
    let (host_ip, rest) = if let Some(stripped) = mapping.strip_prefix('[') {
        let (ip, rest) = stripped.split_once("]:")?;
        (Some(ip.to_owned()), rest)
    } else {
        (None, mapping)
    };
    let parts: Vec<&str> = rest.split(':').collect();
    match (host_ip, parts.as_slice()) {
        (None, [_target]) => Some((None, None, protocol)),
        (None, [published, _target]) => Some((None, Some((*published).to_owned()), protocol)),
        (None, [ip, published, _target]) => Some((Some((*ip).to_owned()), Some((*published).to_owned()), protocol)),
        (Some(ip), [published, _target]) => Some((Some(ip), Some((*published).to_owned()), protocol)),
        _ => None,
    }
}

/// Fail before `docker compose up` when a port the selected services publish
/// is taken by anything other than this Compose project.
// Wired into `forklaunch dev` just before `docker compose up`.
pub(crate) fn preflight(compose: &Value, services: &[String]) -> Result<()> {
    let project = compose.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
    check(&published_ports(compose, services), |port| probe(&project, port))
}

/// The decision, separated from the host probe for tests.
pub(crate) fn check(ports: &[PublishedPort], mut probe: impl FnMut(&PublishedPort) -> Holder) -> Result<()> {
    let mut conflicts = BTreeMap::new();
    for port in ports {
        if let Holder::Other(holder) = probe(port) {
            conflicts.entry((port.port, port.protocol.clone())).or_insert((port.clone(), holder));
        }
    }
    if conflicts.is_empty() {
        return Ok(());
    }
    let mut message = String::from("Published host ports are already in use:\n");
    for (port, holder) in conflicts.values() {
        let address = port.host_ip.as_deref().unwrap_or("0.0.0.0");
        message.push_str(&format!(
            "  - {address}:{}/{} for service `{}` is held by {}\n",
            port.port,
            port.protocol,
            port.service,
            holder.as_deref().unwrap_or("another process")
        ));
    }
    message.push_str(
        "Stop whatever holds them (often another ForkLaunch application's `docker compose down`), \
or change the published host port in docker-compose.yaml (for example `127.0.0.1:6380:6379`).",
    );
    bail!(message)
}

fn probe(project: &str, port: &PublishedPort) -> Holder {
    if let Some((owner, container)) = docker_publisher(port.port) {
        return if !project.is_empty() && owner == project {
            Holder::ThisProject
        } else {
            Holder::Other(Some(if owner.is_empty() {
                format!("container `{container}`")
            } else {
                format!("container `{container}` (Compose project `{owner}`)")
            }))
        };
    }
    let ip = port
        .host_ip
        .as_deref()
        .and_then(|ip| ip.parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    // A wildcard bind can succeed beside a loopback listener; check both.
    let mut addresses = vec![ip];
    if ip.is_unspecified() {
        addresses.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    let taken = addresses.into_iter().any(|ip| {
        let address = SocketAddr::new(ip, port.port);
        if port.protocol.eq_ignore_ascii_case("udp") {
            UdpSocket::bind(address).is_err()
        } else {
            TcpListener::bind(address).is_err()
        }
    });
    if taken { Holder::Other(None) } else { Holder::Free }
}

/// The Compose project and name of a running container publishing `port`.
fn docker_publisher(port: u16) -> Option<(String, String)> {
    let output = Command::new("docker")
        .args(["ps", "--filter", &format!("publish={port}"), "--format"])
        .arg("{{.Label \"com.docker.compose.project\"}}\t{{.Names}}")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    let (project, name) = line.split_once('\t').unwrap_or(("", line));
    Some((project.trim().to_owned(), name.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compose() -> Value {
        json!({
            "name": "demo",
            "services": {
                "redis": {"ports": [{"mode": "ingress", "host_ip": "127.0.0.1", "target": 6379, "published": "6379", "protocol": "tcp"}]},
                "api": {"ports": ["8000:8000", "9229"]},
                "range": {"ports": ["127.0.0.1:7000-7001:7000-7001/udp"]},
                "v6": {"ports": ["[::1]:8443:443"]},
                "internal": {"expose": ["5000"]}
            }
        })
    }

    #[test]
    fn reads_long_short_range_and_ipv6_entries() {
        let ports = published_ports(&compose(), &[]);
        let summary: Vec<_> = ports
            .iter()
            .map(|port| (port.service.as_str(), port.host_ip.as_deref(), port.port, port.protocol.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("redis", Some("127.0.0.1"), 6379, "tcp"),
                ("api", None, 8000, "tcp"),
                ("range", Some("127.0.0.1"), 7000, "udp"),
                ("range", Some("127.0.0.1"), 7001, "udp"),
                ("v6", Some("::1"), 8443, "tcp"),
            ]
        );
    }

    #[test]
    fn selected_services_limit_the_check() {
        let ports = published_ports(&compose(), &["redis".to_owned()]);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 6379);
    }

    #[test]
    fn this_project_and_free_ports_pass() {
        let ports = published_ports(&compose(), &[]);
        check(&ports, |port| if port.port == 6379 { Holder::ThisProject } else { Holder::Free }).unwrap();
    }

    #[test]
    fn conflicts_name_port_service_and_holder() {
        let ports = published_ports(&compose(), &[]);
        let error = check(&ports, |port| match port.port {
            6379 => Holder::Other(Some("container `other-redis-1` (Compose project `other`)".into())),
            8000 => Holder::Other(None),
            _ => Holder::Free,
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("127.0.0.1:6379/tcp for service `redis` is held by container `other-redis-1` (Compose project `other`)"), "{error}");
        assert!(error.contains("0.0.0.0:8000/tcp for service `api` is held by another process"), "{error}");
        assert!(!error.contains("7000"), "{error}");
    }

    #[test]
    fn a_port_bound_on_the_host_is_reported() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let taken = PublishedPort { service: "redis".into(), host_ip: Some("127.0.0.1".into()), port, protocol: "tcp".into() };
        // No container publishes an ephemeral port, so the bind probe decides.
        assert!(matches!(probe("demo", &taken), Holder::Other(None)));
        drop(listener);
        assert_eq!(probe("demo", &taken), Holder::Free);
    }
}
