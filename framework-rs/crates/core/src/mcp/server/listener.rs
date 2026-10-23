//! Shared listener lifecycle for native HTTP and MCP, with atomic startup.
use super::{McpServerOptions, http_transport};
use axum::Router;
use forklaunch_core::contract::ContractRegistry;
use std::{
    future::{Future, IntoFuture},
    io,
    net::SocketAddr,
    time::Duration,
};
use tokio::{net::TcpListener, sync::watch};
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn address(
    mode: Option<&str>,
    host: Option<&str>,
    port: Option<&str>,
    http: SocketAddr,
) -> io::Result<Option<(String, u16)>> {
    let mode = mode.unwrap_or(if host.is_some() || port.is_some() {
        "separate"
    } else {
        "shared"
    });
    match mode {
        "shared" => {
            if host.is_some() || port.is_some() {
                return Err(invalid("MCP_HOST/MCP_PORT require a separate listener"));
            }
            Ok(None)
        }
        "separate" => {
            let host = host.map_or_else(|| http.ip().to_string(), str::to_owned);
            if host.is_empty()
                || host.len() > 253
                || host.chars().any(|ch| ch.is_whitespace() || ch.is_control())
            {
                return Err(invalid("MCP_HOST must be a hostname or IP address"));
            }
            let port = match port {
                Some(value) => value
                    .parse()
                    .map_err(|_| invalid("MCP_PORT must be between 0 and 65535"))?,
                None => http
                    .port()
                    .checked_add(2000)
                    .ok_or_else(|| invalid("HTTP port + 2000 exceeds 65535; configure MCP_PORT"))?,
            };
            Ok(Some((host, port)))
        }
        _ => Err(invalid("MCP_LISTENER must be shared or separate")),
    }
}

/// Serve optional MCP with the same native HTTP handlers and application layers.
/// Server-owned environment controls enablement, endpoint, allowlists and listener.
/// Both listeners bind before either accepts work; shutdown drains both within 30s.
/// # Errors
/// Rejects malformed configuration, occupied listeners, invalid contracts and shutdown failures.
pub async fn serve_http<F, L>(
    router: Router,
    registry: &ContractRegistry,
    mut options: McpServerOptions,
    listener: TcpListener,
    shutdown: F,
    layer: L,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send,
    L: Fn(Router) -> Router,
{
    let enabled: String = crate::config::required("MCP_ENABLED")?;
    match enabled.as_str() {
        "false" | "0" => return serve_listeners(listener, layer(router), None, shutdown).await,
        "true" | "1" => {}
        _ => return Err(invalid("MCP_ENABLED must be true, false, 1 or 0")),
    }
    if let Ok(path) = std::env::var("MCP_PATH") {
        options.path = path;
    }
    for (variable, hosts) in [("MCP_ALLOWED_HOSTS", true), ("MCP_ALLOWED_ORIGINS", false)] {
        if let Ok(value) = std::env::var(variable) {
            let values: Vec<_> = value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
            if values.is_empty() {
                return Err(invalid(format!("{variable} must not be empty")));
            }
            if hosts {
                options.transport = options.transport.with_allowed_hosts(values);
            } else {
                options.transport = options.transport.with_allowed_origins(values);
            }
        }
    }
    let separate = address(
        std::env::var("MCP_LISTENER").ok().as_deref(),
        std::env::var("MCP_HOST").ok().as_deref(),
        std::env::var("MCP_PORT").ok().as_deref(),
        listener.local_addr()?,
    )?;
    let (http, mcp) = if let Some((host, port)) = separate {
        let mcp_listener = tokio::time::timeout(
            Duration::from_secs(5),
            TcpListener::bind((host.as_str(), port)),
        )
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP listener bind timed out"))??;
        let transport =
            http_transport(layer(router.clone()), registry, options).map_err(invalid)?;
        println!("MCP listening on http://{}", mcp_listener.local_addr()?);
        (layer(router), Some((mcp_listener, layer(transport))))
    } else {
        // MCP dispatches through the guarded HTTP clone, and its public surface
        // receives those same application layers independently.
        let transport =
            http_transport(layer(router.clone()), registry, options).map_err(invalid)?;
        (layer(router.merge(transport)), None)
    };
    serve_listeners(listener, http, mcp, shutdown).await
}
async fn stopped(mut receiver: watch::Receiver<bool>) {
    while !*receiver.borrow_and_update() {
        if receiver.changed().await.is_err() {
            break;
        }
    }
}
async fn serve_listeners<F>(
    listener: TcpListener,
    http: Router,
    separate: Option<(TcpListener, Router)>,
    shutdown: F,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send,
{
    let (stop, receiver) = watch::channel(false);
    let http = axum::serve(listener, http)
        .with_graceful_shutdown(stopped(receiver.clone()))
        .into_future();
    let has_mcp = separate.is_some();
    let mcp = async move {
        if let Some((listener, router)) = separate {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopped(receiver))
                .await
        } else {
            std::future::pending::<io::Result<()>>().await
        }
    };
    tokio::pin!(http, mcp, shutdown);
    let mut http_done = false;
    let mut mcp_done = !has_mcp;
    let result = tokio::select! {
        () = &mut shutdown => Ok(()),
        result = &mut http => { http_done=true; result },
        result = &mut mcp, if has_mcp => { mcp_done=true; result },
    };
    stop.send_replace(true);
    let drained = tokio::time::timeout(Duration::from_secs(30), async {
        let (http, mcp) = tokio::join!(
            async { if http_done { Ok(()) } else { http.await } },
            async { if mcp_done { Ok(()) } else { mcp.await } },
        );
        http.and(mcp)
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "HTTP/MCP shutdown exceeded 30 seconds",
        )
    })?;
    result.and(drained)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listener_configuration_matches_typescript_port_offset_and_validates_bounds() {
        let http: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        assert_eq!(address(None, None, None, http).unwrap(), None);
        assert_eq!(
            address(Some("separate"), None, None, http)
                .unwrap()
                .unwrap()
                .1,
            10000
        );
        assert_eq!(address(None, None, Some("0"), http).unwrap().unwrap().1, 0);
        assert_eq!(
            address(None, Some("::1"), Some("1234"), http)
                .unwrap()
                .unwrap(),
            ("::1".to_owned(), 1234)
        );
        assert!(address(Some("shared"), None, Some("1234"), http).is_err());
        assert!(address(Some("invalid"), None, None, http).is_err());
        assert!(address(None, Some("invalid host"), None, http).is_err());
        assert!(address(None, None, Some("65536"), http).is_err());
        assert!(
            address(
                Some("separate"),
                None,
                None,
                "127.0.0.1:64000".parse().unwrap()
            )
            .is_err()
        );
    }
}
