# WebSocket contracts and managed bindings

`forklaunch_ws` exposes the core-owned socket contracts and codecs alongside the
native socket transport. With `typed-schema`, `#[socket_api]` declares a complete
socket set without importing the server runtime into the contract package:

```rust,ignore
#[forklaunch_ws::socket_api]
pub mod sockets {
    use super::*;
    pub fn presence() -> TypedSocket<NoBody, PresenceInput, PresenceOutput> {
        TypedSocket::new(
            RouteContract::get("Presence", "Presence", "/presence")
                .with_access(AccessLevel::Authenticated),
            "presence-input", "presence-output",
        ).unwrap()
    }
}

let registry = sockets::registry()?;
let app = sockets::Bindings {
    presence: sockets::Presence::bind_authorized(policy, stream_options, handler),
}.mount(ContractRouter::new(registry))?.finish()?;
```

Every declaration requires its own tagged binding. Omissions and swapped socket
identities fail compilation, even for identical wire types. Mounting also checks
the handler's handshake, incoming and outgoing message types. Each socket keeps
its explicit policy and bounded `StreamOptions`; the adapter delegates to the
same managed handshake, frame validation, periodic/per-message authorization,
backpressure and shutdown path as individual `bind_socket_authorized` calls.

The generated registry contains socket metadata, not finite HTTP responses or
MCP tools. The runtime still checks dynamic path conflicts and exact published
schemas. Contracts can describe encrypted envelopes; assembling static bindings
does not alter envelope encryption, authenticated tenant/purpose context, key
rotation or the application's audited decoding policy. Those remain enforced by
the existing channel codec and policy configured for that socket.
