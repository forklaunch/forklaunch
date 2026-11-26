#!/usr/bin/env python3
"""Compile the reviewed generated SDK and exercise transport admission and redirect refusal."""
from pathlib import Path
import os
import subprocess
import tempfile

framework = Path(__file__).resolve().parents[4]
repo = framework.parent
with tempfile.TemporaryDirectory(prefix="sdk-transport-proof-") as directory:
    root = Path(directory)
    (root / "Cargo.toml").write_text('''[package]
name = "sdk-transport-proof"
version = "0.0.0"
edition = "2024"
[lib]
path = "lib.rs"
[dependencies]
reqwest = { version = "0.13", default-features = false, features = ["json", "query", "rustls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "=1.0.151"
forklaunch-universal-sdk = { path = "''' + str(framework / "crates/universal-sdk") + '''" }
forklaunch-core = { path = "''' + str(framework / "crates/core") + '''", features = ["schema-validation"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "net", "io-util", "time"] }
''')
    (root / "sdk.rs").write_text((framework / "crates/core/src/typescript/snapshots/catalog.rust.snap").read_text())
    (root / "lib.rs").write_text('''#[allow(dead_code)] mod sdk;
#[cfg(test)] mod tests {
use super::sdk::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[test]
fn schema_typed_metadata_serializes_as_wire_scalars() {
    let request = ForkLaunchClient::new("http://localhost")
        .get_item_request(GetItemRequest {
            params: GetItemParams { id: "note 3".into() },
            query: GetItemQuery { include_details: Some(true) },
            headers: GetItemHeaders { x_request_id: Some("revision-3".into()) },
            ..Default::default()
        }).unwrap();
    assert!(request.url.ends_with("/items/note%203"));
    assert_eq!(request.query["includeDetails"], "true");
    assert_eq!(request.headers["x-request-id"], "revision-3");
    let empty = ForkLaunchClient::new("http://localhost").get_item_request(GetItemRequest::default()).unwrap();
    assert!(empty.query.is_empty());
    assert!(empty.headers.is_empty());
}
#[tokio::test]
async fn same_status_responses_select_media_specific_typed_variants() {
    for media in ["text/plain", "text/html", "text/csv", "application/pdf", "application/xml"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            socket.read(&mut bytes).await.unwrap();
            socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: {media}; charset=utf-8\\r\\nContent-Length: 5\\r\\nConnection: close\\r\\n\\r\\nhello").as_bytes()).await.unwrap();
        });
        let response = ForkLaunchClient::new(format!("http://{address}")).get_item(GetItemRequest::default()).await;
        if media == "application/xml" { assert!(matches!(response,Err(SdkError::Contract(_)))); server.await.unwrap(); continue; }
        match (media, response.unwrap()) {
            ("text/plain", GetItemResponses::Status200TextPlain(value)) => assert_eq!(value, "hello"),
            ("text/html", GetItemResponses::Status200TextHtml(value)) => assert_eq!(value, "hello"),
            ("text/csv", GetItemResponses::Status200TextCsv(value)) => assert_eq!(value, "hello"),
            ("application/pdf", GetItemResponses::Status200ApplicationPdf(value)) => assert_eq!(value, b"hello"),

            _ => panic!("incorrect representation variant"),
        }
        server.await.unwrap();
    }
}
#[tokio::test]
async fn response_headers_are_typed_and_required() {
    for complete in [true,false] {
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {
            let (mut socket,_)=listener.accept().await.unwrap();
            let mut request=[0;8192];socket.read(&mut request).await.unwrap();
            let body=r#"{"id":"one","name":"note"}"#;
            let headers=if complete {"etag: rev1\\r\\nx-count: 7\\r\\n"} else {""};
            socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\n{headers}Content-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{body}",body.len()).as_bytes()).await.unwrap();
        });
        let result=ForkLaunchClient::new(format!("http://{address}")).get_item(GetItemRequest::default()).await;
        if complete {
            let GetItemResponses::Status200ApplicationJson(value)=result.unwrap() else {panic!("wrong variant")};
            assert_eq!(value.headers.etag,"rev1");assert_eq!(value.headers.x_count,7);assert_eq!(value.headers.x_next,None);
            assert_eq!(value.body.id,"one");
        } else {assert!(matches!(result,Err(SdkError::ResponseHeaders)));}
        server.await.unwrap();
    }
}
async fn fixture_response(body: &str, media: &str, headers: &str) -> (ForkLaunchClient, tokio::task::JoinHandle<()>) {
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address=listener.local_addr().unwrap();
    let wire=format!("HTTP/1.1 200 OK\\r\\nContent-Type: {media}\\r\\n{headers}Content-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{body}",body.len());
    let server=tokio::spawn(async move {
        let (mut socket,_)=listener.accept().await.unwrap();
        let mut request=[0;8192];socket.read(&mut request).await.unwrap();
        socket.write_all(wire.as_bytes()).await.unwrap();
    });
    (ForkLaunchClient::new(format!("http://{address}")),server)
}
#[tokio::test]
async fn rejects_constraints_even_when_serde_can_decode() {
    for (body,count) in [(r#"{"id":"one","name":""}"#, "2"),
        (r#"{"id":"one","name":"note","unexpected":true}"#, "2"),
        (r#"{"id":"one","name":"note"}"#, "-1"),
        (r#"{"id":"one","name":"note"}"#, "1.5"),
        ("private-malformed-payload", "2")] {
        let (client,server)=fixture_response(body,"application/json",&format!("etag: rev1\\r\\nx-count: {count}\\r\\n")).await;
        let error=client.get_item(GetItemRequest::default()).await.unwrap_err();
        assert!(matches!(error,SdkError::Contract(_)|SdkError::ResponseHeaders));
        assert!(!error.to_string().contains(body));server.await.unwrap();
    }
}
#[tokio::test]
async fn ambiguous_content_type_is_rejected() {
    let (client,server)=fixture_response(r#"{"id":"one","name":"note"}"#,"application/json","Content-Type: text/plain\\r\\n").await;
    assert!(matches!(client.get_item(GetItemRequest::default()).await,Err(SdkError::Contract(_))));
    server.await.unwrap();
}
#[tokio::test]
async fn native_payload_streams_validate_and_close_on_bad_events() {
    for (body,valid) in [(r#"id: revision
data: {"id":"native"}

"#,true),
        (r#"data: {"id":""}

"#,false),("data: not-json\\n\\n",false)] {
        let (client,server)=fixture_response(body,"text/event-stream","").await;
        let NativeEventsResponses::Status200(mut stream)=client.native_events(NativeEventsRequest::default()).await.unwrap() else {panic!("wrong stream")};
        if valid {let event=stream.next().await.unwrap().unwrap();assert_eq!(event.data.id,"native");assert_eq!(event.id.as_deref(),Some("revision"));stream.close();}
        else {assert!(matches!(stream.next().await,Err(SdkError::Contract(_))));}
        assert!(stream.next().await.unwrap().is_none());server.await.unwrap();
    }
}
#[tokio::test]
async fn secure_admission_precedes_network_and_redirects_never_follow() {
    let denied = ForkLaunchClient::new("http://example.test").get_item(GetItemRequest::default()).await;
    assert!(matches!(denied, Err(SdkError::TransportPolicy)));
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = origin.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = origin.accept().await.unwrap();
        let mut request = [0; 8192];
        socket.read(&mut request).await.unwrap();
        socket.write_all(format!("HTTP/1.1 302 Found\\r\\nLocation: http://{target_addr}/redirected\\r\\nContent-Length: 0\\r\\nConnection: close\\r\\n\\r\\n").as_bytes()).await.unwrap();
    });
    let response = ForkLaunchClient::new(format!("http://{addr}"))
        .with_bearer_token("private-test-token").get_item(GetItemRequest::default()).await.unwrap();
    assert!(matches!(response, GetItemResponses::Status302(())));
    server.await.unwrap();
    assert!(tokio::time::timeout(std::time::Duration::from_millis(100), target.accept()).await.is_err());
    let _explicit = ForkLaunchClient::new("http://notes:8000").with_plaintext_development_transport();
}
}
''')
    environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_TARGET_DIR=str(repo / "cli/target"))
    command = "cargo +stable test --offline --quiet --manifest-path " + __import__("shlex").quote(str(root / "Cargo.toml"))
    runner = ["bash", "-c", command] if os.environ.get("FORKLAUNCH_WORKER_RUNTIME_LOCKED") == "1" else ["python3", str(repo / "cli/tests/fixtures/worker_runtime_lock.py"), "-c", command]
    subprocess.run(runner, env=environment, check=True)
