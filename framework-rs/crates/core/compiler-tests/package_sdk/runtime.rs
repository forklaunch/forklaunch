#[allow(dead_code)]
#[path = "../empty/lib.rs"]
pub mod empty;
#[allow(dead_code, unused_imports)]
mod included {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixture/generated.rs"));
}
use split_sdk_consumer::{FetchResponses, ForkLaunchClient, Item};
use std::io::{Read, Write};
fn server(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8192];
        let _ = stream.read(&mut request).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    (format!("http://{address}"), task)
}
#[tokio::test]
async fn split_public_facade_validates_remote_values() {
    let _empty = empty::ForkLaunchClient::new("http://127.0.0.1");
    let _included = included::ForkLaunchClient::new("http://127.0.0.1");
    let _: Item = serde_json::from_str(r#"{"name":"valid"}"#).unwrap();
    for (body, valid) in [
        (r#"{"name":"valid"}"#, true),
        (r#"{"name":"x"}"#, false),
        (r#"{"name":4}"#, false),
    ] {
        let (url, task) = server(body);
        let response = ForkLaunchClient::new(url)
            .fetch()
            .await;
        if valid {
            assert!(matches!(response,Ok(FetchResponses::Status200(value)) if value.name=="valid"));
        } else {
            assert!(response.is_err());
        }
        task.join().unwrap();
    }
}
