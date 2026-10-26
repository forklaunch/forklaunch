use super::*;
use axum::body::to_bytes;
use futures_util::{TryStreamExt, stream};
use serde_json::{Value, json};
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

struct Source {
    polls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    first: bool,
}
impl Stream for Source {
    type Item = Result<SseFrame<Value>, ()>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if self.first {
            self.first = false;
            Poll::Ready(Some(Ok(SseFrame::new("first", json!({"message":"ready"})))))
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn headers_wait_for_one_event_and_body_drop_cancels_without_prefetch() {
    let polls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let response = respond_with(
        Source {
            polls: polls.clone(),
            drops: drops.clone(),
            first: true,
        },
        SseOptions::default(),
        |_| true,
    )
    .await
    .unwrap();
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    let mut body = response.into_body().into_data_stream();
    assert_eq!(
        body.try_next().await.unwrap().unwrap(),
        "id: first\ndata: {\"message\":\"ready\"}\n\n"
    );
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    let mut next = Box::pin(body.try_next());
    assert!(futures_util::poll!(&mut next).is_pending());
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    drop(next);
    drop(body);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn first_event_errors_are_http_errors_and_later_errors_are_terminal() {
    let input = |values: Vec<Value>| {
        stream::iter(
            values
                .into_iter()
                .map(|data| Ok::<_, ()>(SseFrame::new("event", data))),
        )
    };
    let validate = |frame: &SseFrame<Value>| frame.data.is_string();
    let error = respond_with(input(vec![json!(42)]), SseOptions::default(), validate)
        .await
        .unwrap_err();
    assert_eq!(error, SseEmitError::InvalidEvent);
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let response = respond_with(
        input(vec![json!("ok"), json!(42), json!("never")]),
        SseOptions::default(),
        validate,
    )
    .await
    .unwrap();
    let mut body = response.into_body().into_data_stream();
    assert!(body.try_next().await.unwrap().is_some());
    assert!(body.try_next().await.is_err());
    assert!(body.try_next().await.unwrap().is_none());
    let response = respond_with(
        input(vec![json!("ok"), json!(42)]),
        SseOptions {
            validation: SseValidation::FirstEvent,
            ..SseOptions::default()
        },
        validate,
    )
    .await
    .unwrap();
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("data: 42\n"));
    let error = respond_with(
        stream::iter(vec![Err::<SseFrame<Value>, _>("private backend detail")]),
        SseOptions::default(),
        |_| true,
    )
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "SSE response producer failed");
}
#[tokio::test]
async fn framing_limits_and_empty_streams_apply_independently_of_validation_policy() {
    let response = respond_with(
        stream::empty::<Result<SseFrame<Value>, ()>>(),
        SseOptions::default(),
        |_| panic!("no event"),
    )
    .await
    .unwrap();
    assert!(to_bytes(response.into_body(), 64).await.unwrap().is_empty());
    assert!(
        respond_with(
            stream::empty::<Result<SseFrame<Value>, ()>>(),
            SseOptions {
                max_event_bytes: 0,
                ..SseOptions::default()
            },
            |_| true
        )
        .await
        .is_err()
    );
    assert_eq!(
        encode(
            &SseFrame::new("id\ndata: injected", json!("ok")),
            4096,
            &|_| true
        ),
        Err(SseEmitError::InvalidEvent)
    );
    assert_eq!(
        encode(&SseFrame::new("", json!("x".repeat(64))), 16, &|_| true),
        Err(SseEmitError::EventTooLarge)
    );
    let bytes = encode(
        &SseFrame::new("", json!("first\r\nsecond\nthird")),
        4096,
        &|_| true,
    )
    .unwrap();
    assert_eq!(bytes, b"id: \ndata: first\ndata: second\ndata: third\n\n");
    let escaped = SseFrame::new("", json!("\t".repeat(32)));
    let encoded = encode(&escaped, 4096, &|_| true).unwrap();
    assert!(encode(&escaped, encoded.len(), &|_| true).is_ok());
    let frame = SseFrame::new("unicode", json!("水 🦀"));
    let bytes = encode(&frame, 4096, &|_| true).unwrap();
    assert!(encode(&frame, bytes.len(), &|_| true).is_ok());
    assert_eq!(
        encode(&frame, bytes.len() - 1, &|_| true),
        Err(SseEmitError::EventTooLarge)
    );
}
#[test]
fn actual_typescript_emitter_fixture() {
    let Ok(input) = std::env::var("FORKLAUNCH_TEST_SSE_EMITTER_INPUT") else {
        return;
    };
    let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let values: Vec<_> = cases
        .into_iter()
        .map(|case| {
            let frame = SseFrame::new(case["id"].as_str().unwrap(), case["data"].clone());
            String::from_utf8(encode(&frame, 4096, &|_| true).unwrap()).unwrap()
        })
        .collect();
    std::fs::write(
        std::env::var("FORKLAUNCH_TEST_SSE_EMITTER_OUTPUT").unwrap(),
        serde_json::to_vec(&values).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn framework_schema_validation_runs_on_typed_data() {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, garde::Validate)]
    struct Data {
        #[garde(length(min = 1))]
        message: String,
    }
    let source = stream::iter(vec![Ok::<_, ()>(SseFrame::new(
        "first",
        Data {
            message: String::new(),
        },
    ))]);
    assert_eq!(
        respond(source, SseOptions::default()).await.unwrap_err(),
        SseEmitError::InvalidEvent
    );
    let source = stream::iter(vec![Ok::<_, ()>(SseFrame::new(
        "first",
        Data {
            message: "valid".into(),
        },
    ))]);
    assert!(respond(source, SseOptions::default()).await.is_ok());
}

#[tokio::test]
async fn cancelling_before_first_event_drops_the_producer() {
    let polls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let source = Source {
        polls: polls.clone(),
        drops: drops.clone(),
        first: false,
    };
    let mut response = Box::pin(respond_with(source, SseOptions::default(), |_| true));
    assert!(futures_util::poll!(&mut response).is_pending());
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    drop(response);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
