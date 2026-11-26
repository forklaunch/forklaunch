# {{{name}}}

Rust CAC module with the upstream free mock procedure-code provider. No licensed
CPT data or clinical entities are included. Configure `HMAC_SECRET_KEY` and a
trusted `JWKS_PUBLIC_KEY_URL`, then run `cargo run -p {{{name}}}`.

The crate follows the generated service layout. `config.rs` reads trusted settings, `registrations.rs` builds the provider and `AppState`, and `api/routes/<resource>.rs` binds each CAC contract to the controller in `api/controllers/<resource>.rs`. Controllers are the module's own code: each extracts its request, calls the code-set provider and maps the result to the contract's responses, so edit one to customize that operation. Routers added with `forklaunch init router` live in `api/routes/extensions`.

`GET /codeSet` reports the provider descriptor. `GET /codeSet/:code` returns one
of the three existing mock codes or a text 404. Both require TypeScript-compatible
request-bound HMAC authorization with key ID `default` and a timestamp within
five minutes. Clients must sign the exact method/path/body bytes. Distributed
nonce replay rejection remains an application policy.

`GET /compliance/export/:userId` and `DELETE /compliance/erase/:userId` require a
verified asymmetric JWT and the `system` role. The current upstream skeleton
registers no entities, so authorized requests return the matching text 404.
There is no database migration or fabricated clinical storage. Roles default to
verified issuer claims; call `with_role_resolver` on the `HmacJwksAuth` that
`config.rs` builds to surface
roles from trusted IAM. The TypeScript template's admin-role placeholder is not
a production authorization source. Optional `JWT_ISSUER`/`JWT_AUDIENCE` constrain
the configured issuer further. Tokens require an expiry and honor `nbf`.

`GET /health` is public. Contracts, OpenAPI and both SDK targets use the mounted
route registry. Offline artifact export requires no keys, JWKS network access or
telemetry configuration. Shutdown drains HTTP work and flushes optional OTLP.
User source and Cargo metadata survive sync/reinit and module rename.

HTTP health and Streamable HTTP MCP share the configured listener. MCP is served at `/mcp` by default, and tools retain HTTP route authentication. `MCP_ENABLED` (true or false) is required; optionally configure `MCP_PATH`, `MCP_ALLOWED_HOSTS` and `MCP_ALLOWED_ORIGINS` for deployment.
