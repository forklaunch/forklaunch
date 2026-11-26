use super::*;
use axum::http::request::Parts;
use forklaunch_core::{
    contract::{AccessLevel, endpoint::NoBody},
    encryption::{EnvelopeKey, EnvelopeProfile},
};
use forklaunch_ws::{
    codec::PayloadCodec,
    transport::{SocketEvent, SocketOptions, SocketSchemas, connect},
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug)]
pub struct CipherFrame {
    ciphertext: String,
}
#[forklaunch_ws::socket_api]
mod channels {
    use super::*;
    pub fn secure() -> forklaunch_ws::TypedSocket<NoBody, CipherFrame, CipherFrame> {
        forklaunch_ws::TypedSocket::new(
            RouteContract::get("Secure", "Secure", "/secure")
                .with_access(AccessLevel::Authenticated),
            "encrypted-request",
            "encrypted-response",
        )
        .unwrap()
    }
}
#[derive(Clone)]
struct Policy {
    tenant: Uuid,
    allowed: Arc<AtomicBool>,
}
#[allow(clippy::unused_async_trait_impl)]
impl EndpointPolicy<()> for Policy {
    type Context = Uuid;
    async fn authorize(
        &self,
        _: &mut Parts,
        (): &(),
        _: &RouteContract,
    ) -> Result<Uuid, EndpointError> {
        if self.allowed.load(Ordering::SeqCst) {
            Ok(self.tenant)
        } else {
            Err(EndpointError::Forbidden)
        }
    }
}
fn key(active: &str, material: u8, previous: Vec<(String, [u8; 32])>) -> EnvelopeKey {
    EnvelopeKey::with_rotation(
        EnvelopeProfile::new("static-sockets").unwrap(),
        active.into(),
        [material; 32],
        previous,
        None,
    )
    .unwrap()
}
fn stream_options(shutdown: Shutdown) -> StreamOptions {
    StreamOptions::new(
        shutdown,
        4096,
        Duration::from_millis(10),
        Duration::from_millis(100),
    )
    .unwrap()
}
#[tokio::test]
async fn static_socket_encrypted_rotation_context_rejection_and_live_revocation() {
    for invalid_context in [false, true] {
        let tenant = Uuid::new_v4();
        let resource = Uuid::new_v4();
        let policy = Policy {
            tenant,
            allowed: Arc::new(AtomicBool::new(true)),
        };
        let keys = Arc::new(key("active", 9, vec![("old".into(), [7; 32])]));
        let applied = Arc::new(AtomicUsize::new(0));
        let count = applied.clone();
        let handler_key = keys.clone();
        let bindings = channels::Bindings {
            secure: channels::Secure::bind_authorized(
                policy.clone(),
                stream_options(Shutdown::new()),
                move |_: State<()>,
                      _: Uuid,
                      (),
                      mut session: SocketSession<Uuid, CipherFrame, CipherFrame>| {
                    let key = handler_key.clone();
                    let count = count.clone();
                    async move {
                        while let Some((context, frame)) = session.recv().await {
                            let Ok(plain) =
                                key.open(context, resource, "request", &frame.ciphertext)
                            else {
                                return;
                            };
                            count.fetch_add(1, Ordering::SeqCst);
                            let ciphertext =
                                key.seal(context, resource, "response", &plain).unwrap();
                            if session.send(CipherFrame { ciphertext }).await.is_err() {
                                return;
                            }
                        }
                    }
                },
            ),
        };
        let app = bindings
            .mount(ContractRouter::<()>::new(channels::registry().unwrap()))
            .unwrap()
            .finish()
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut client = connect(
            format!("ws://{address}/secure"),
            SocketSchemas::<CipherFrame, CipherFrame>::new(
                PayloadCodec::serde().with_json_encoding(),
                PayloadCodec::serde().with_json_encoding(),
            ),
            SocketOptions::default(),
        )
        .await
        .unwrap();
        let frame = encrypted_request(
            if invalid_context {
                Uuid::new_v4()
            } else {
                tenant
            },
            resource,
        );
        client.send(&frame).await.unwrap();
        let response = tokio::time::timeout(Duration::from_secs(2), client.recv())
            .await
            .unwrap();
        if invalid_context {
            assert!(!matches!(response, Ok(Some(SocketEvent::Message(_)))));
            assert_eq!(applied.load(Ordering::SeqCst), 0);
        } else {
            let Ok(Some(SocketEvent::Message(frame))) = response else {
                panic!("missing encrypted response")
            };
            assert!(frame.ciphertext.starts_with("v3.active."));
            assert_eq!(
                keys.open(tenant, resource, "response", &frame.ciphertext)
                    .unwrap()
                    .as_slice(),
                b"PRIVATE_SOCKET_CANARY"
            );
            assert!(
                keys.open(tenant, resource, "request", &frame.ciphertext)
                    .is_err()
            );
            assert_eq!(applied.load(Ordering::SeqCst), 1);
            policy.allowed.store(false, Ordering::SeqCst);
            let closed = tokio::time::timeout(Duration::from_secs(2), client.recv())
                .await
                .unwrap();
            assert!(!matches!(closed, Ok(Some(SocketEvent::Message(_)))));
        }
        server.abort();
    }
}

fn encrypted_request(tenant: Uuid, resource: Uuid) -> CipherFrame {
    let frame = CipherFrame {
        ciphertext: key("old", 7, vec![])
            .seal(tenant, resource, "request", b"PRIVATE_SOCKET_CANARY")
            .unwrap(),
    };
    assert!(
        !serde_json::to_string(&frame)
            .unwrap()
            .contains("PRIVATE_SOCKET_CANARY")
    );
    frame
}
