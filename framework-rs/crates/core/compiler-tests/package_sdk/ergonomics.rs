use sdk_ergonomics::{
    Client, EchoRequest, EchoResponses, GetHealthRequest, GetHealthResponses, Health,
};
use std::io::{Read, Write};

fn server(body: &'static str, expected_header: bool) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8192];
        let count = stream.read(&mut request).unwrap();
        let request = std::str::from_utf8(&request[..count]).unwrap();
        if expected_header {
            assert!(request.to_lowercase().contains("x-trace: fixture"));
        }
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    (format!("http://{address}"), task)
}

#[tokio::test]
async fn no_argument_and_options_calls_validate_unsigned_remote_values() {
    for (body, valid) in [
        (r#"{"recordCount":0}"#, true),
        (r#"{"recordCount":9007199254740991}"#, true),
        (r#"{"recordCount":-1}"#, false),
        (r#"{"recordCount":9007199254740992}"#, false),
        (r#"{"recordCount":"1"}"#, false),
        (r#"{"recordCount":1.5}"#, false),
    ] {
        let (url, task) = server(body, false);
        let response = Client::new(url).get_health().await;
        if valid {
            assert!(matches!(response, Ok(GetHealthResponses::Status200(_))));
        } else {
            assert!(response.is_err());
        }
        task.join().unwrap();
    }
    let (url, task) = server(r#"{"recordCount":1}"#, true);
    let result = Client::new(url)
        .get_health_with_options(GetHealthRequest {
            headers: [("x-trace".into(), "fixture".into())].into(),
        })
        .await
        .unwrap();
    assert!(matches!(result, GetHealthResponses::Status200(value) if value.record_count == 1));
    task.join().unwrap();
}

#[tokio::test]
async fn typed_body_call_still_uses_the_checked_runtime() {
    let (url, task) = server(r#"{"recordCount":2}"#, false);
    let health: Health = serde_json::from_str(r#"{"recordCount":2}"#).unwrap();
    let response = Client::new(url)
        .echo(EchoRequest {
            body: Some(health),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(matches!(response, EchoResponses::Status200(value) if value.record_count == 2));
    task.join().unwrap();
}
