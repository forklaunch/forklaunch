#![cfg(feature = "transport")]
use std::time::Duration;

use forklaunch_ws::{
    codec::PayloadCodec,
    transport::{
        self, SocketError, SocketEvent, SocketOptions, SocketSchemas, TypedWebSocketServer,
        handshake, http,
    },
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

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
        PayloadCodec::serde().with_validator(|request: &Request| {
            if request.message.is_empty() {
                Err("message is empty".into())
            } else {
                Ok(())
            }
        }),
        PayloadCodec::serde(),
    )
    .with_ping(PayloadCodec::serde().with_validator(|stamp: &Stamp| {
        if stamp.ts == 0 {
            Err("timestamp is zero".into())
        } else {
            Ok(())
        }
    }))
    .with_pong(PayloadCodec::serde())
    .with_close(PayloadCodec::serde())
}
#[allow(clippy::result_large_err)] // Required by tungstenite's upgrade callback signature.
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

async fn serve_authorized_peer(server: TypedWebSocketServer<Request, Reply, Stamp, Stamp, Reason>) {
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
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Ping(Stamp { ts: 42 }))
    );
    socket.pong(Some(&Stamp { ts: 43 })).await.unwrap();
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
}

#[tokio::test]
async fn typed_peers_authorize_validate_control_frames_and_complete_close_handshakes() {
    let options = SocketOptions::default();
    let server = TypedWebSocketServer::bind("127.0.0.1:0", schemas(), options)
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let task = tokio::spawn(serve_authorized_peer(server));
    assert!(
        transport::connect(
            format!("ws://{address}/chat"),
            schemas().reversed(),
            options
        )
        .await
        .is_err()
    );
    let mut request = format!("ws://{address}/chat")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer fixture-token".parse().unwrap());
    let mut client = transport::connect(request, schemas().reversed(), options)
        .await
        .unwrap();
    assert!(matches!(
        client
            .send(&Request {
                message: String::new()
            })
            .await,
        Err(SocketError::Payload(_))
    ));
    assert!(matches!(
        client.ping(Some(&Stamp { ts: 0 })).await,
        Err(SocketError::Payload(_))
    ));
    client
        .send(&Request {
            message: "hello 水".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        client.recv().await.unwrap(),
        Some(SocketEvent::Message(Reply {
            accepted: "水 🦀".into()
        }))
    );
    client.ping(Some(&Stamp { ts: 42 })).await.unwrap();
    assert_eq!(
        client.recv().await.unwrap(),
        Some(SocketEvent::Pong(Stamp { ts: 42 }))
    );
    assert_eq!(
        client.recv().await.unwrap(),
        Some(SocketEvent::Pong(Stamp { ts: 43 }))
    );
    assert_eq!(
        client.recv().await.unwrap(),
        Some(SocketEvent::Close {
            code: 1000,
            reason: Reason {
                reason: "done".into()
            }
        })
    );
    assert!(client.recv().await.unwrap().is_none());
    timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn invalid_wire_payloads_close_before_application_delivery_and_release_the_socket() {
    let server = TypedWebSocketServer::bind("127.0.0.1:0", schemas(), SocketOptions::default())
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = server.accept().await.unwrap();
        assert!(matches!(socket.recv().await, Err(SocketError::Payload(_))));
        assert!(socket.recv().await.unwrap().is_none());
    });
    let (mut raw, _) = tokio_tungstenite::connect_async(format!("ws://{address}/"))
        .await
        .unwrap();
    raw.send(Message::binary(br#"{"message":""}"#.to_vec()))
        .await
        .unwrap();
    let close = timeout(Duration::from_secs(3), raw.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(close, Message::Close(Some(frame)) if u16::from(frame.code) == 1008));
    task.await.unwrap();
}

#[tokio::test]
async fn frame_limits_and_handshake_timeouts_are_enforced_on_real_connections() {
    let options = SocketOptions {
        max_message_bytes: 128,
        handshake_timeout: Duration::from_millis(30),
        ..SocketOptions::default()
    };
    let server = TypedWebSocketServer::bind("127.0.0.1:0", schemas(), options)
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let stalled = tokio::net::TcpStream::connect(address).await.unwrap();
    assert!(matches!(
        server.accept().await,
        Err(SocketError::Timeout("handshake"))
    ));
    drop(stalled);
    let task = tokio::spawn(async move {
        let (mut socket, _) = server.accept().await.unwrap();
        assert!(matches!(
            socket.recv().await,
            Err(SocketError::Transport(_))
        ));
        assert!(socket.recv().await.unwrap().is_none());
    });
    let (mut raw, _) = tokio_tungstenite::connect_async(format!("ws://{address}/"))
        .await
        .unwrap();
    raw.send(Message::binary(vec![b'x'; 1024])).await.unwrap();
    task.await.unwrap();
    let _ = timeout(Duration::from_secs(3), raw.next()).await.unwrap();
    assert!(
        TypedWebSocketServer::bind(
            "127.0.0.1:0",
            schemas(),
            SocketOptions {
                max_message_bytes: 0,
                ..options
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn cancelling_a_receive_keeps_the_next_message_and_control_limits_reject_before_send() {
    let string_schemas = SocketSchemas::new(
        PayloadCodec::<String>::serde(),
        PayloadCodec::<String>::serde(),
    )
    .with_ping(PayloadCodec::<String>::serde());
    let server = TypedWebSocketServer::bind(
        "127.0.0.1:0",
        string_schemas.clone(),
        SocketOptions::default(),
    )
    .await
    .unwrap();
    let address = server.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = server.accept().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        socket.send(&"after cancellation".into()).await.unwrap();
        assert!(socket.ping(Some(&"x".repeat(126))).await.is_err());
        assert!(socket.close(1005, None).await.is_err());
        assert!(
            socket
                .close(1000, Some(&serde_json::Value::String("x".repeat(124))))
                .await
                .is_err()
        );
        socket.close(1000, None).await.unwrap();
        let _ = socket.recv().await;
    });
    let mut client = transport::connect(
        format!("ws://{address}/"),
        string_schemas.reversed(),
        SocketOptions::default(),
    )
    .await
    .unwrap();
    assert!(
        timeout(Duration::from_millis(20), client.recv())
            .await
            .is_err()
    );
    assert_eq!(
        client.recv().await.unwrap(),
        Some(SocketEvent::Message("after cancellation".into()))
    );
    assert!(matches!(
        client.recv().await.unwrap(),
        Some(SocketEvent::Close { code: 1000, .. })
    ));
    task.await.unwrap();
}

#[tokio::test]
async fn stalled_writes_time_out_and_release_the_transport() {
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::protocol::{Role, WebSocketConfig},
    };
    let (writer, mut reader) = tokio::io::duplex(64);
    let raw = WebSocketStream::from_raw_socket(
        writer,
        Role::Server,
        Some(
            WebSocketConfig::default()
                .max_message_size(Some(1024))
                .max_frame_size(Some(1024))
                .write_buffer_size(0)
                .max_write_buffer_size(1152),
        ),
    )
    .await;
    let mut socket = transport::TypedWebSocket::from_socket(
        raw,
        SocketSchemas::new(
            PayloadCodec::<String>::serde(),
            PayloadCodec::<String>::serde(),
        ),
        SocketOptions {
            max_message_bytes: 1024,
            write_timeout: Duration::from_millis(30),
            ..SocketOptions::default()
        },
    )
    .unwrap();
    assert!(matches!(
        socket.send(&"x".repeat(512)).await,
        Err(SocketError::Timeout("write"))
    ));
    assert!(socket.recv().await.unwrap().is_none());
    assert!(matches!(
        socket.send(&"later".into()).await,
        Err(SocketError::Closed)
    ));
    let mut remaining = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut remaining)
        .await
        .unwrap();
    assert!(remaining.len() <= 64);
}

#[tokio::test]
async fn automatic_pong_must_satisfy_its_outgoing_schema() {
    let server = TypedWebSocketServer::bind(
        "127.0.0.1:0",
        schemas().with_pong(PayloadCodec::<String>::serde()),
        SocketOptions::default(),
    )
    .await
    .unwrap();
    let address = server.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = server.accept().await.unwrap();
        assert!(matches!(socket.recv().await, Err(SocketError::Payload(_))));
        assert!(socket.recv().await.unwrap().is_none());
    });
    let (mut raw, _) = tokio_tungstenite::connect_async(format!("ws://{address}/"))
        .await
        .unwrap();
    raw.send(Message::Ping(br#"{"ts":42}"#.to_vec().into()))
        .await
        .unwrap();
    assert!(
        matches!(raw.next().await.unwrap().unwrap(), Message::Close(Some(frame)) if u16::from(frame.code) == 1008)
    );
    task.await.unwrap();
}

#[tokio::test]
async fn cancelling_a_partial_write_releases_the_connection() {
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::protocol::{Role, WebSocketConfig},
    };
    let (writer, mut reader) = tokio::io::duplex(64);
    let raw = WebSocketStream::from_raw_socket(
        writer,
        Role::Server,
        Some(
            WebSocketConfig::default()
                .max_message_size(Some(1024))
                .max_frame_size(Some(1024))
                .write_buffer_size(0)
                .max_write_buffer_size(1152),
        ),
    )
    .await;
    let mut socket = transport::TypedWebSocket::from_socket(
        raw,
        SocketSchemas::new(
            PayloadCodec::<String>::serde(),
            PayloadCodec::<String>::serde(),
        ),
        SocketOptions {
            max_message_bytes: 1024,
            ..SocketOptions::default()
        },
    )
    .unwrap();
    assert!(
        timeout(Duration::from_millis(30), socket.send(&"x".repeat(512)))
            .await
            .is_err()
    );
    assert!(socket.recv().await.unwrap().is_none());
    assert!(matches!(
        socket.send(&"later".into()).await,
        Err(SocketError::Closed)
    ));
    let mut remaining = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut remaining)
        .await
        .unwrap();
    assert!(remaining.len() <= 64);
}
