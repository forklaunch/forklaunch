//! Bounded, non-destructive observation of opt-in Warning response validators.
use super::{FrameworkBody, FrameworkRequest, FrameworkResponse, Registration, RouteContract};
use axum::{body::Body, response::Response};
use bytes::Bytes;
use http_body::{Body as HttpBody, Frame};
use http_body_util::BodyExt;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

struct Replay {
    frames: VecDeque<Result<Frame<Bytes>, axum::Error>>,
    remaining: Body,
}
impl HttpBody for Replay {
    type Data = Bytes;
    type Error = axum::Error;
    fn is_end_stream(&self) -> bool {
        self.frames.is_empty() && self.remaining.is_end_stream()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        let queued = self
            .frames
            .iter()
            .filter_map(|frame| frame.as_ref().ok()?.data_ref())
            .fold(0_u64, |size, bytes| size.saturating_add(bytes.len() as u64));
        let remaining = self.remaining.size_hint();
        let mut hint = http_body::SizeHint::new();
        hint.set_lower(remaining.lower().saturating_add(queued));
        if let Some(upper) = remaining
            .upper()
            .and_then(|upper| upper.checked_add(queued))
        {
            hint.set_upper(upper);
        }
        hint
    }
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        if let Some(frame) = self.frames.pop_front() {
            return Poll::Ready(Some(frame));
        }
        Pin::new(&mut self.remaining).poll_frame(cx)
    }
}

pub(super) async fn validate(
    entry: &Registration,
    route: &RouteContract,
    request: &FrameworkRequest,
    response: Response,
) -> Response {
    let (parts, mut body) = response.into_parts();
    let mut frames = VecDeque::new();
    let mut bytes = Vec::new();
    let deadline = tokio::time::Instant::now() + entry.timeout;
    let reason = loop {
        let frame = match tokio::time::timeout_at(deadline, body.frame()).await {
            Err(_) => break Some("collection_timeout"),
            Ok(None) => break None,
            Ok(Some(frame)) => frame,
        };
        let mut failure = None;
        match &frame {
            Ok(frame) => {
                if let Some(data) = frame.data_ref() {
                    if data.len() > entry.max_bytes.saturating_sub(bytes.len()) {
                        failure = Some("collection_limit");
                    } else {
                        bytes.extend_from_slice(data);
                    }
                }
            }
            Err(_) => failure = Some("stream_error"),
        }
        frames.push_back(frame);
        if failure.is_some() {
            break failure;
        }
        if frames.len() >= 1_024 {
            break Some("frame_limit");
        }
    };
    let reason = reason.or_else(|| match std::str::from_utf8(&bytes) {
        Err(_) => Some("invalid_utf8"),
        Ok(text) => {
            let candidate = FrameworkResponse {
                status_code: parts.status.as_u16(),
                headers: parts
                    .headers
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .to_str()
                            .ok()
                            .map(|value| (name.to_string(), value.to_owned()))
                    })
                    .collect(),
                body: if bytes.is_empty() {
                    FrameworkBody::Empty
                } else {
                    FrameworkBody::Text(text.to_owned())
                },
            };
            (entry.validate)(
                request,
                &candidate,
                parts
                    .extensions
                    .get::<forklaunch_core::http::MatchedVersionResponses>(),
            )
            .map(|_| "schema_rejection")
        }
    });
    if let Some(reason) = reason {
        tracing::warn!(operation = %route.operation_name, correlation_id = %request.context.correlation_id, reason, "response schema observation failed; original response retained");
    }
    Response::from_parts(
        parts,
        Body::new(Replay {
            frames,
            remaining: body,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResponseValidators;
    use forklaunch_core::contract::{BodyContract, HttpMethod, ValidationMode};
    use forklaunch_core::http::RequestContext;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    fn route() -> RouteContract {
        let mut route = RouteContract::get("Warning", "Warning", "/warning")
            .with_response(200, Some(BodyContract::json("Output")));
        route.options.response_validation = ValidationMode::Warning;
        route
    }
    fn request() -> FrameworkRequest {
        FrameworkRequest::new(HttpMethod::Get, "/warning", RequestContext::new("warning"))
    }
    #[tokio::test]
    async fn explicit_warning_observes_once_without_replacement_or_header_changes() {
        let route = route();
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = calls.clone();
        let mut validators = ResponseValidators::default();
        assert!(validators.enable_warning(&route).is_err());
        validators
            .insert(&route, 64, Duration::from_secs(1), move |_, response| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(response.body.as_text(), Some("original"));
                Some(FrameworkResponse::text(500, "PRIVATE_DIAGNOSTIC"))
            })
            .unwrap();
        validators.enable_warning(&route).unwrap();
        for _ in 0..2 {
            let mut response = Response::new(Body::from("original"));
            response
                .headers_mut()
                .append("set-cookie", "a=1".parse().unwrap());
            response
                .headers_mut()
                .append("set-cookie", "b=2".parse().unwrap());
            response.extensions_mut().insert(42_u64);
            let response = validators.validate(&route, &request(), response).await;
            assert_eq!(response.status(), 200);
            assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
            assert_eq!(response.extensions().get::<u64>(), Some(&42));
            assert_eq!(
                response.into_body().collect().await.unwrap().to_bytes(),
                "original"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let mut none = route.clone();
        none.options.response_validation = ValidationMode::None;
        validators
            .validate(&none, &request(), Response::new(Body::from("skip")))
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
    struct Frames(VecDeque<Result<Frame<Bytes>, std::io::Error>>);
    impl HttpBody for Frames {
        type Data = Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, std::io::Error>>> {
            Poll::Ready(self.0.pop_front())
        }
    }
    #[tokio::test]
    async fn limits_and_errors_replay_original_frames_and_trailers() {
        let route = route();
        let mut validators = ResponseValidators::default();
        validators
            .insert(&route, 2, Duration::from_secs(1), |_, _| {
                panic!("bounded failure reached schema")
            })
            .unwrap();
        validators.enable_warning(&route).unwrap();
        let mut trailers = axum::http::HeaderMap::new();
        trailers.insert("x-trailer", "kept".parse().unwrap());
        let stream = Frames(VecDeque::from([
            Ok(Frame::data(Bytes::from_static(b"a"))),
            Ok(Frame::data(Bytes::from_static(b"bc"))),
            Ok(Frame::trailers(trailers.clone())),
        ]));
        let response = validators
            .validate(&route, &request(), Response::new(Body::new(stream)))
            .await;
        let collected = response.into_body().collect().await.unwrap();
        assert_eq!(collected.trailers(), Some(&trailers));
        assert_eq!(collected.to_bytes(), "abc");
        let stream = Frames(VecDeque::from([
            Ok(Frame::data(Bytes::from_static(b"a"))),
            Err(std::io::Error::other("PRIVATE_STREAM_ERROR")),
        ]));
        let response = validators
            .validate(&route, &request(), Response::new(Body::new(stream)))
            .await;
        let mut body = response.into_body();
        assert_eq!(
            body.frame().await.unwrap().unwrap().into_data().unwrap(),
            "a"
        );
        assert!(body.frame().await.unwrap().is_err());
        assert!(body.frame().await.is_none());
    }
    struct Delayed {
        delay: Pin<Box<tokio::time::Sleep>>,
        sent: bool,
    }
    impl HttpBody for Delayed {
        type Data = Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, std::io::Error>>> {
            use std::future::Future;
            if self.sent {
                return Poll::Ready(None);
            }
            if self.delay.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.sent = true;
            Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"later")))))
        }
    }
    #[tokio::test]
    async fn timeout_and_invalid_utf8_leave_original_body_readable() {
        let route = route();
        let mut validators = ResponseValidators::default();
        validators
            .insert(&route, 16, Duration::from_millis(1), |_, _| {
                panic!("unreadable body reached schema")
            })
            .unwrap();
        validators.enable_warning(&route).unwrap();
        let response = validators
            .validate(
                &route,
                &request(),
                Response::new(Body::new(Delayed {
                    delay: Box::pin(tokio::time::sleep(Duration::from_millis(20))),
                    sent: false,
                })),
            )
            .await;
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            "later"
        );
        let response = validators
            .validate(&route, &request(), Response::new(Body::from(vec![255_u8])))
            .await;
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            &[255]
        );
    }
}
