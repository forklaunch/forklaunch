//! Typed client/server sockets with validated messages and bounded handshakes/writes.
use std::{
    fmt,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{
        self,
        handshake::server::{Callback, NoCallback},
        protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
    },
};

use crate::codec::{DEFAULT_MAX_PAYLOAD_BYTES, PayloadCodec, PayloadError};
pub use tokio_tungstenite::tungstenite::client::IntoClientRequest;
pub use tokio_tungstenite::tungstenite::handshake::client::Response as ClientHandshakeResponse;
pub use tokio_tungstenite::tungstenite::protocol::{
    CloseFrame as ProtocolCloseFrame, frame::coding::CloseCode as ProtocolCloseCode,
};
pub use tokio_tungstenite::tungstenite::{Message, handshake, http};

#[derive(Debug)]
pub enum SocketError {
    Payload(PayloadError),
    Transport(Box<tungstenite::Error>),
    Io(std::io::Error),
    Timeout(&'static str),
    Configuration(&'static str),
    Closed,
}
impl fmt::Display for SocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Payload(error) => error.fmt(f),
            Self::Transport(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
            Self::Timeout(operation) => write!(f, "WebSocket {operation} timed out"),
            Self::Configuration(message) => write!(f, "invalid WebSocket configuration: {message}"),
            Self::Closed => f.write_str("WebSocket is closed"),
        }
    }
}
impl std::error::Error for SocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Payload(error) => Some(error),
            Self::Transport(error) => Some(error.as_ref()),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
impl From<PayloadError> for SocketError {
    fn from(error: PayloadError) -> Self {
        Self::Payload(error)
    }
}
impl From<tungstenite::Error> for SocketError {
    fn from(error: tungstenite::Error) -> Self {
        Self::Transport(Box::new(error))
    }
}
impl From<std::io::Error> for SocketError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SocketOptions {
    pub max_message_bytes: usize,
    pub handshake_timeout: Duration,
    pub write_timeout: Duration,
}
impl Default for SocketOptions {
    fn default() -> Self {
        Self {
            max_message_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            handshake_timeout: Duration::from_secs(10),
            write_timeout: Duration::from_secs(5),
        }
    }
}
impl SocketOptions {
    /// Bounded protocol configuration shared by native and HTTP upgrade adapters.
    ///
    /// # Errors
    /// Rejects zero limits/timeouts and buffer-size overflow.
    pub fn config(self) -> Result<WebSocketConfig, SocketError> {
        if self.max_message_bytes == 0
            || self.handshake_timeout.is_zero()
            || self.write_timeout.is_zero()
        {
            return Err(SocketError::Configuration(
                "limits and timeouts must be positive",
            ));
        }
        let write_limit = self
            .max_message_bytes
            .checked_add(128)
            .ok_or(SocketError::Configuration("message limit is too large"))?;
        Ok(WebSocketConfig::default()
            .read_buffer_size(self.max_message_bytes.min(16384))
            .write_buffer_size(0)
            .max_write_buffer_size(write_limit)
            .max_frame_size(Some(self.max_message_bytes))
            .max_message_size(Some(self.max_message_bytes)))
    }
}

/// Incoming/outgoing types are from the current endpoint's perspective.
/// `reversed()` swaps message directions for the peer, retaining control schemas.
pub struct SocketSchemas<I, O, P = Value, Q = Value, C = Value> {
    pub incoming: PayloadCodec<I>,
    pub outgoing: PayloadCodec<O>,
    pub ping: PayloadCodec<P>,
    pub pong: PayloadCodec<Q>,
    pub close: PayloadCodec<C>,
}
impl<I, O> SocketSchemas<I, O> {
    #[must_use]
    pub fn new(incoming: PayloadCodec<I>, outgoing: PayloadCodec<O>) -> Self {
        Self {
            incoming,
            outgoing,
            ping: PayloadCodec::serde(),
            pong: PayloadCodec::serde(),
            close: PayloadCodec::serde(),
        }
    }
}
impl<I, O, P, Q, C> Clone for SocketSchemas<I, O, P, Q, C> {
    fn clone(&self) -> Self {
        Self {
            incoming: self.incoming.clone(),
            outgoing: self.outgoing.clone(),
            ping: self.ping.clone(),
            pong: self.pong.clone(),
            close: self.close.clone(),
        }
    }
}
impl<I, O, P, Q, C> SocketSchemas<I, O, P, Q, C> {
    #[must_use]
    pub fn reversed(self) -> SocketSchemas<O, I, P, Q, C> {
        SocketSchemas {
            incoming: self.outgoing,
            outgoing: self.incoming,
            ping: self.ping,
            pong: self.pong,
            close: self.close,
        }
    }
    #[must_use]
    pub fn with_ping<T>(self, ping: PayloadCodec<T>) -> SocketSchemas<I, O, T, Q, C> {
        SocketSchemas {
            incoming: self.incoming,
            outgoing: self.outgoing,
            ping,
            pong: self.pong,
            close: self.close,
        }
    }
    #[must_use]
    pub fn with_pong<T>(self, pong: PayloadCodec<T>) -> SocketSchemas<I, O, P, T, C> {
        SocketSchemas {
            incoming: self.incoming,
            outgoing: self.outgoing,
            ping: self.ping,
            pong,
            close: self.close,
        }
    }
    #[must_use]
    pub fn with_close<T>(self, close: PayloadCodec<T>) -> SocketSchemas<I, O, P, Q, T> {
        SocketSchemas {
            incoming: self.incoming,
            outgoing: self.outgoing,
            ping: self.ping,
            pong: self.pong,
            close,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SocketEvent<I, P = Value, Q = Value, C = Value> {
    Message(I),
    Ping(P),
    Pong(Q),
    Close { code: u16, reason: C },
}

/// A frame transport whose implementation enforces the declared protocol bounds.
/// Adapters must validate frame/message/write limits before application delivery.
pub trait FrameTransport:
    Stream<Item = Result<Message, SocketError>> + Sink<Message, Error = SocketError> + Unpin
{
    /// # Errors
    /// Rejects transport limits that exceed the application's configuration.
    fn validate_limits(&self, options: SocketOptions) -> Result<(), SocketError>;
}

/// Native TCP/TLS transport. Construct through `TypedWebSocket::from_socket`.
pub struct NativeTransport<S> {
    socket: WebSocketStream<S>,
}
impl<S: AsyncRead + AsyncWrite + Unpin> Stream for NativeTransport<S> {
    type Item = Result<Message, SocketError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.socket)
            .poll_next(cx)
            .map(|message| message.map(|result| result.map_err(SocketError::from)))
    }
}
impl<S: AsyncRead + AsyncWrite + Unpin> Sink<Message> for NativeTransport<S> {
    type Error = SocketError;
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.socket)
            .poll_ready(cx)
            .map_err(SocketError::from)
    }
    fn start_send(mut self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        Pin::new(&mut self.socket)
            .start_send(message)
            .map_err(SocketError::from)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.socket)
            .poll_flush(cx)
            .map_err(SocketError::from)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.socket)
            .poll_close(cx)
            .map_err(SocketError::from)
    }
}
impl<S: AsyncRead + AsyncWrite + Unpin> FrameTransport for NativeTransport<S> {
    fn validate_limits(&self, options: SocketOptions) -> Result<(), SocketError> {
        let expected = options.config()?;
        let actual = self.socket.get_config();
        if actual
            .max_message_size
            .is_none_or(|limit| limit > options.max_message_bytes)
            || actual
                .max_frame_size
                .is_none_or(|limit| limit > options.max_message_bytes)
            || actual.max_write_buffer_size > expected.max_write_buffer_size
        {
            return Err(SocketError::Configuration(
                "upgraded socket must have bounded message/frame/write limits",
            ));
        }
        Ok(())
    }
}

/// Typed messages and controls over a bounded frame transport.
pub struct TypedSocket<T, I, O, P = Value, Q = Value, C = Value> {
    socket: Option<T>,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
}
/// Typed native TCP/TLS connection, preserving the stream-oriented API.
pub type TypedWebSocket<S, I, O, P = Value, Q = Value, C = Value> =
    TypedSocket<NativeTransport<S>, I, O, P, Q, C>;
impl<S: AsyncRead + AsyncWrite + Unpin, I, O, P, Q, C>
    TypedSocket<NativeTransport<S>, I, O, P, Q, C>
{
    /// Wrap an already-upgraded native socket.
    ///
    /// # Errors
    /// Rejects unbounded/preconfigured sockets whose limits exceed the supplied options.
    pub fn from_socket(
        socket: WebSocketStream<S>,
        schemas: SocketSchemas<I, O, P, Q, C>,
        options: SocketOptions,
    ) -> Result<Self, SocketError> {
        Self::from_transport(NativeTransport { socket }, schemas, options)
    }
}
impl<T: FrameTransport, I, O, P, Q, C> TypedSocket<T, I, O, P, Q, C> {
    /// Wrap an adapter after checking its configured protocol bounds.
    ///
    /// # Errors
    /// Rejects invalid options or transport limits exceeding them.
    pub fn from_transport(
        socket: T,
        schemas: SocketSchemas<I, O, P, Q, C>,
        options: SocketOptions,
    ) -> Result<Self, SocketError> {
        options.config()?;
        socket.validate_limits(options)?;
        Ok(Self {
            socket: Some(socket),
            schemas,
            options,
        })
    }

    /// # Errors
    /// Rejects invalid/oversized outgoing values, closed sockets and write failures/timeouts.
    pub async fn send(&mut self, value: &O) -> Result<(), SocketError> {
        self.write(Message::binary(self.schemas.outgoing.encode(value)?))
            .await
    }

    /// # Errors
    /// Rejects invalid/oversized control values and write failures/timeouts.
    pub async fn ping(&mut self, value: Option<&P>) -> Result<(), SocketError> {
        let bytes =
            value.map_or_else(|| Ok(Vec::new()), |value| self.schemas.ping.encode(value))?;
        control_limit(bytes.len(), 125)?;
        self.write(Message::Ping(bytes.into())).await
    }

    /// # Errors
    /// Rejects invalid/oversized control values and write failures/timeouts.
    pub async fn pong(&mut self, value: Option<&Q>) -> Result<(), SocketError> {
        let bytes =
            value.map_or_else(|| Ok(Vec::new()), |value| self.schemas.pong.encode(value))?;
        control_limit(bytes.len(), 125)?;
        self.write(Message::Pong(bytes.into())).await
    }

    /// Initiate a close handshake; keep receiving until the peer close event or EOF.
    ///
    /// # Errors
    /// Rejects reserved codes, invalid/oversized/non-UTF-8 reasons and write failures/timeouts.
    pub async fn close(&mut self, code: u16, reason: Option<&C>) -> Result<(), SocketError> {
        let code = CloseCode::from(code);
        if !code.is_allowed() {
            return Err(SocketError::Configuration("reserved close code"));
        }
        let bytes =
            reason.map_or_else(|| Ok(Vec::new()), |value| self.schemas.close.encode(value))?;
        control_limit(bytes.len(), 123)?;
        let reason =
            String::from_utf8(bytes).map_err(|error| PayloadError::Invalid(error.to_string()))?;
        self.write(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await
    }

    /// Receive and validate one application/control event. Invalid incoming payloads
    /// close the connection with policy code 1008 and never reach application handlers.
    ///
    /// # Errors
    /// Returns payload, protocol and transport errors. A failed connection is released.
    pub async fn recv(&mut self) -> Result<Option<SocketEvent<I, P, Q, C>>, SocketError> {
        loop {
            let Some(socket) = self.socket.as_mut() else {
                return Ok(None);
            };
            let message = match socket.next().await {
                None => {
                    self.socket.take();
                    return Ok(None);
                }
                Some(Err(error)) => {
                    self.socket.take();
                    return Err(error);
                }
                Some(Ok(message)) => message,
            };
            let event = match message {
                Message::Text(text) => self
                    .schemas
                    .incoming
                    .decode(text.as_bytes())
                    .map(SocketEvent::Message),
                Message::Binary(bytes) => self
                    .schemas
                    .incoming
                    .decode(&bytes)
                    .map(SocketEvent::Message),
                Message::Ping(bytes) => self.schemas.pong.decode(&bytes).and_then(|_| {
                    // Tungstenite queues an exact-byte pong automatically. Validate that
                    // outgoing control payload before allowing flush to transmit it.
                    self.schemas.ping.decode(&bytes).map(SocketEvent::Ping)
                }),
                Message::Pong(bytes) => self.schemas.pong.decode(&bytes).map(SocketEvent::Pong),
                Message::Close(frame) => {
                    let (code, reason) = frame.map_or((1005, Vec::new()), |frame| {
                        (u16::from(frame.code), frame.reason.as_bytes().to_vec())
                    });
                    let reason = self.schemas.close.decode(&reason);
                    match reason {
                        Ok(reason) => {
                            self.flush().await?;
                            self.socket.take();
                            return Ok(Some(SocketEvent::Close { code, reason }));
                        }
                        Err(error) => {
                            self.reject().await;
                            return Err(error.into());
                        }
                    }
                }
                Message::Frame(_) => continue,
            };
            match event {
                Ok(event) => {
                    self.flush().await?;
                    return Ok(Some(event));
                }
                Err(error) => {
                    self.reject().await;
                    return Err(error.into());
                }
            }
        }
    }

    async fn write(&mut self, message: Message) -> Result<(), SocketError> {
        if message.len() > self.options.max_message_bytes {
            return Err(PayloadError::Limit {
                max_bytes: self.options.max_message_bytes,
            }
            .into());
        }
        // Ownership stays in this future while writing: cancellation releases a
        // partially written connection instead of retaining queued application data.
        let mut socket = self.socket.take().ok_or(SocketError::Closed)?;
        match timeout(self.options.write_timeout, socket.send(message)).await {
            Ok(Ok(())) => {
                self.socket = Some(socket);
                Ok(())
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(SocketError::Timeout("write")),
        }
    }
    async fn flush(&mut self) -> Result<(), SocketError> {
        let mut socket = self.socket.take().ok_or(SocketError::Closed)?;
        match timeout(self.options.write_timeout, socket.flush()).await {
            Ok(Ok(())) => {
                self.socket = Some(socket);
                Ok(())
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(SocketError::Timeout("flush")),
        }
    }
    async fn reject(&mut self) {
        if let Some(mut socket) = self.socket.take() {
            let frame = CloseFrame {
                code: CloseCode::Policy,
                reason: "invalid payload".into(),
            };
            let _ = timeout(
                self.options.write_timeout,
                socket.send(Message::Close(Some(frame))),
            )
            .await;
        }
    }
}
fn control_limit(bytes: usize, max_bytes: usize) -> Result<(), PayloadError> {
    if bytes > max_bytes {
        Err(PayloadError::Limit { max_bytes })
    } else {
        Ok(())
    }
}

/// Connect with optional caller-supplied handshake headers/subprotocols.
///
/// # Errors
/// Rejects invalid options, handshake/transport failures and handshake timeouts.
pub async fn connect<R: IntoClientRequest + Unpin, I, O, P, Q, C>(
    request: R,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
) -> Result<TypedWebSocket<MaybeTlsStream<TcpStream>, I, O, P, Q, C>, SocketError> {
    Ok(connect_with_response(request, schemas, options).await?.0)
}

/// Connect and retain the upgrade response, including negotiated subprotocols.
///
/// # Errors
/// Returns the same configuration, handshake, timeout and transport errors as `connect`.
pub async fn connect_with_response<R: IntoClientRequest + Unpin, I, O, P, Q, C>(
    request: R,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
) -> Result<
    (
        TypedWebSocket<MaybeTlsStream<TcpStream>, I, O, P, Q, C>,
        ClientHandshakeResponse,
    ),
    SocketError,
> {
    let config = options.config()?;
    let request = request.into_client_request()?;
    require_secure_endpoint(request.uri())?;
    let (socket, response) = timeout(
        options.handshake_timeout,
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| SocketError::Timeout("handshake"))??;
    Ok((
        TypedWebSocket::from_socket(socket, schemas, options)?,
        response,
    ))
}

fn require_secure_endpoint(uri: &http::Uri) -> Result<(), SocketError> {
    if uri
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'))
        || !matches!(uri.scheme_str(), Some("wss" | "ws"))
    {
        return Err(SocketError::Configuration(
            "Socket endpoints must be ws or wss without credentials",
        ));
    }
    Ok(())
}

/// Accept a stream after the application has authorized the connection.
///
/// # Errors
/// Rejects invalid options, handshake/transport failures and handshake timeouts.
pub async fn accept<S: AsyncRead + AsyncWrite + Unpin, I, O, P, Q, C>(
    stream: S,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
) -> Result<TypedWebSocket<S, I, O, P, Q, C>, SocketError> {
    accept_with(stream, schemas, options, NoCallback).await
}

/// Authorize/check the HTTP upgrade request before accepting a typed socket.
///
/// # Errors
/// Returns callback rejections, malformed handshakes, invalid options and timeouts.
pub async fn accept_with<S: AsyncRead + AsyncWrite + Unpin, I, O, P, Q, C, F: Callback + Unpin>(
    stream: S,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
    callback: F,
) -> Result<TypedWebSocket<S, I, O, P, Q, C>, SocketError> {
    let config = options.config()?;
    let socket = timeout(
        options.handshake_timeout,
        tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config)),
    )
    .await
    .map_err(|_| SocketError::Timeout("handshake"))??;
    TypedWebSocket::from_socket(socket, schemas, options)
}

/// An application-owned listener; socket tasks and shutdown remain explicit.
pub struct TypedWebSocketServer<I, O, P = Value, Q = Value, C = Value> {
    listener: TcpListener,
    schemas: SocketSchemas<I, O, P, Q, C>,
    options: SocketOptions,
}
impl<I, O, P, Q, C> TypedWebSocketServer<I, O, P, Q, C> {
    /// # Errors
    /// Rejects invalid options or listener binding failures.
    pub async fn bind(
        address: impl ToSocketAddrs,
        schemas: SocketSchemas<I, O, P, Q, C>,
        options: SocketOptions,
    ) -> Result<Self, SocketError> {
        options.config()?;
        Ok(Self {
            listener: TcpListener::bind(address).await?,
            schemas,
            options,
        })
    }
    /// # Errors
    /// Returns the underlying listener's address errors.
    pub fn local_addr(&self) -> Result<SocketAddr, SocketError> {
        Ok(self.listener.local_addr()?)
    }

    /// # Errors
    /// Returns accept/handshake failures or timeouts. Use `accept_with` for authorization.
    pub async fn accept(
        &self,
    ) -> Result<(TypedWebSocket<TcpStream, I, O, P, Q, C>, SocketAddr), SocketError> {
        self.accept_with(NoCallback).await
    }

    /// # Errors
    /// Returns listener/handshake failures, callback rejections and timeouts.
    pub async fn accept_with<F: Callback + Unpin>(
        &self,
        callback: F,
    ) -> Result<(TypedWebSocket<TcpStream, I, O, P, Q, C>, SocketAddr), SocketError> {
        let (stream, address) = self.listener.accept().await?;
        let socket = accept_with(stream, self.schemas.clone(), self.options, callback).await?;
        Ok((socket, address))
    }
}

#[cfg(test)]
mod transport_admission_tests {
    use super::*;
    #[test]
    fn socket_endpoints_reject_credentials_and_other_schemes() {
        // Internal service networks use plain ws; credentials never ride in the URI.
        for endpoint in [
            "wss://user:secret@example.test/socket",
            "ws://user@example.test/socket",
            "http://example.test/socket",
        ] {
            assert!(require_secure_endpoint(&endpoint.parse().unwrap()).is_err());
        }
        for endpoint in [
            "wss://example.test/socket",
            "ws://example.test/socket",
            "ws://localhost:8000/socket",
            "ws://127.0.0.1:8000/socket",
            "ws://[::1]:8000/socket",
        ] {
            assert!(require_secure_endpoint(&endpoint.parse().unwrap()).is_ok());
        }
    }
}
