//! Bidirectional interoperability fixture using the real TypeScript WebSocket wrappers.
use forklaunch_ws::{
    codec::PayloadCodec,
    transport::{
        self, SocketEvent, SocketOptions, SocketSchemas, TypedWebSocketServer, handshake, http,
    },
};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Request {
    message: String,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Reply {
    accepted: String,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Stamp {
    ts: u64,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Reason {
    reason: String,
}
fn schemas() -> SocketSchemas<Request, Reply, Stamp, Stamp, Reason> {
    SocketSchemas::new(
        PayloadCodec::serde().with_validator(|value: &Request| {
            if value.message.is_empty() {
                Err("empty message".into())
            } else {
                Ok(())
            }
        }),
        PayloadCodec::serde(),
    )
    .with_ping(PayloadCodec::serde().with_validator(|value: &Stamp| {
        if value.ts == 0 {
            Err("zero timestamp".into())
        } else {
            Ok(())
        }
    }))
    .with_pong(PayloadCodec::serde())
    .with_close(PayloadCodec::serde())
}
#[allow(clippy::result_large_err)] // The protocol's callback requires this signature.
fn authorized(
    request: &handshake::server::Request,
    response: handshake::server::Response,
) -> Result<handshake::server::Response, handshake::server::ErrorResponse> {
    if request.uri().path() != "/chat"
        || request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            != Some("Bearer fixture-token")
    {
        return Err(http::Response::builder()
            .status(401)
            .body(Some("unauthorized".into()))
            .unwrap());
    }
    Ok(response)
}
async fn server() {
    let server = TypedWebSocketServer::bind("127.0.0.1:0", schemas(), SocketOptions::default())
        .await
        .unwrap();
    println!("ws://{}/chat", server.local_addr().unwrap());
    assert!(server.accept_with(authorized).await.is_err());
    let (mut socket, _) = server.accept_with(authorized).await.unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Message(Request {
            message: "hello 水".into()
        }))
    );
    socket
        .send(&Reply {
            accepted: "水 🦀".into(),
        })
        .await
        .unwrap();
    socket.ping(Some(&Stamp { ts: 42 })).await.unwrap();
    for ts in [42, 43] {
        assert_eq!(
            socket.recv().await.unwrap(),
            Some(SocketEvent::Pong(Stamp { ts }))
        );
    }
    socket
        .close(
            1000,
            Some(&Reason {
                reason: "done".into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Close {
            code: 1000,
            reason: Reason {
                reason: "done".into()
            }
        })
    );
    assert!(socket.recv().await.unwrap().is_none());
    println!("PASS native server with actual TypeScript client");
}
async fn client(url: String) {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer fixture-token".parse().unwrap());
    let mut socket = transport::connect(request, schemas().reversed(), SocketOptions::default())
        .await
        .unwrap();
    assert!(
        socket
            .send(&Request {
                message: String::new()
            })
            .await
            .is_err()
    );
    assert!(socket.ping(Some(&Stamp { ts: 0 })).await.is_err());
    socket
        .send(&Request {
            message: "hello 水".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Message(Reply {
            accepted: "水 🦀".into()
        }))
    );
    socket.ping(Some(&Stamp { ts: 42 })).await.unwrap();
    for ts in [42, 43] {
        assert_eq!(
            socket.recv().await.unwrap(),
            Some(SocketEvent::Pong(Stamp { ts }))
        );
    }
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Close {
            code: 1000,
            reason: Reason {
                reason: "done".into()
            }
        })
    );
    assert!(socket.recv().await.unwrap().is_none());
    println!("PASS native client with actual TypeScript server");
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        match args.get(1).map(String::as_str) {
            Some("server") => server().await,
            Some("client") => client(args[2].clone()).await,
            _ => panic!("usage: websocket_runtime server | client URL"),
        }
    })
    .await
    .expect("WebSocket interoperability timed out");
}
