//! Typed WebSocket upgrades on an application's existing HTTP listener.
//!
//! Authorize the HTTP request before calling [`upgrade`]. Router composition and
//! graceful cancellation of connection tasks remain application-owned.
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    extract::ws::{Message as AxumMessage, OnFailedUpgrade, WebSocket, WebSocketUpgrade},
    response::Response,
};
use forklaunch_ws::transport::{
    FrameTransport, Message, ProtocolCloseCode, ProtocolCloseFrame, SocketError, SocketOptions,
    SocketSchemas, TypedSocket,
};
use futures_util::{Sink, Stream};
use serde_json::Value;

/// An upgraded Axum transport with limits set by [`upgrade`].
pub struct AxumTransport {
    socket: WebSocket,
    options: SocketOptions,
}
/// Validated typed connection sharing the native runtime's message/control behavior.
pub type AxumTypedSocket<I, O, P = Value, Q = Value, C = Value> =
    TypedSocket<AxumTransport, I, O, P, Q, C>;

/// Upgrade an already-authorized HTTP request into a bounded typed connection.
/// Caller-selected subprotocols and upgrade-failure callbacks are preserved.
/// HTTP server handshake timeouts apply before this upgrade; `write_timeout`
/// bounds socket writes afterward. Callers own connection lifetime and shutdown.
///
/// # Errors
/// Rejects invalid protocol limits before issuing an HTTP upgrade response.
pub fn upgrade<I, O, P, Q, C, F, Fut, E>(
    ws: WebSocketUpgrade<E>,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
    handler: F,
) -> Result<Response, SocketError>
where
    E: OnFailedUpgrade,
    I: Send + 'static,
    O: Send + 'static,
    P: Send + 'static,
    Q: Send + 'static,
    C: Send + 'static,
    F: FnOnce(AxumTypedSocket<I, O, P, Q, C>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let config = options.config()?;
    Ok(ws
        .read_buffer_size(config.read_buffer_size)
        .write_buffer_size(config.write_buffer_size)
        .max_write_buffer_size(config.max_write_buffer_size)
        .max_message_size(options.max_message_bytes)
        .max_frame_size(options.max_message_bytes)
        .on_upgrade(move |socket| async move {
            match TypedSocket::from_transport(AxumTransport { socket, options }, schemas, options) {
                Ok(socket) => handler(socket).await,
                Err(error) => {
                    tracing::error!(%error, "typed WebSocket adapter configuration failed");
                }
            }
        }))
}
impl FrameTransport for AxumTransport {
    fn validate_limits(&self, options: SocketOptions) -> Result<(), SocketError> {
        self.options.config()?;
        if self.options.max_message_bytes > options.max_message_bytes {
            return Err(SocketError::Configuration(
                "upgraded socket exceeds message/frame/write limits",
            ));
        }
        Ok(())
    }
}
fn transport_error(error: axum::Error) -> SocketError {
    std::io::Error::other(error).into()
}
fn incoming(message: AxumMessage) -> Message {
    match message {
        AxumMessage::Text(text) => Message::text(text.as_str()),
        AxumMessage::Binary(bytes) => Message::Binary(bytes),
        AxumMessage::Ping(bytes) => Message::Ping(bytes),
        AxumMessage::Pong(bytes) => Message::Pong(bytes),
        AxumMessage::Close(frame) => Message::Close(frame.map(|frame| ProtocolCloseFrame {
            code: ProtocolCloseCode::from(frame.code),
            reason: frame.reason.as_str().into(),
        })),
    }
}
fn outgoing(message: Message) -> Result<AxumMessage, SocketError> {
    Ok(match message {
        Message::Text(text) => AxumMessage::Text(text.as_str().into()),
        Message::Binary(bytes) => AxumMessage::Binary(bytes),
        Message::Ping(bytes) => AxumMessage::Ping(bytes),
        Message::Pong(bytes) => AxumMessage::Pong(bytes),
        Message::Close(frame) => {
            AxumMessage::Close(frame.map(|frame| axum::extract::ws::CloseFrame {
                code: u16::from(frame.code),
                reason: frame.reason.as_str().into(),
            }))
        }
        Message::Frame(_) => {
            return Err(SocketError::Configuration(
                "raw protocol frames are not supported by the HTTP adapter",
            ));
        }
    })
}
impl Stream for AxumTransport {
    type Item = Result<Message, SocketError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.socket)
            .poll_next(cx)
            .map(|message| message.map(|result| result.map(incoming).map_err(transport_error)))
    }
}
impl Sink<Message> for AxumTransport {
    type Error = SocketError;
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.socket)
            .poll_ready(cx)
            .map_err(transport_error)
    }
    fn start_send(mut self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        Pin::new(&mut self.socket)
            .start_send(outgoing(message)?)
            .map_err(transport_error)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.socket)
            .poll_flush(cx)
            .map_err(transport_error)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.socket)
            .poll_close(cx)
            .map_err(transport_error)
    }
}
