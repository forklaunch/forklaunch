# A local framework checkout ([framework] path in .forklaunch/manifest.toml)
# arrives as the `framework` and `blueprint` build contexts that
# docker-compose.yaml passes; published crates need neither and these stay empty.
# Only workspace manifests and crates/ are copied, never the checkout's target/.
FROM scratch AS framework
FROM scratch AS blueprint

{{{builder_stage}}}
COPY --from=framework Cargo.tom[l] Cargo.loc[k] ${FRAMEWORK_ROOT}/framework-rs/
COPY --from=framework crate[s] ${FRAMEWORK_ROOT}/framework-rs/crates
COPY --from=blueprint Cargo.tom[l] Cargo.loc[k] ${FRAMEWORK_ROOT}/blueprint-rs/
COPY --from=blueprint crate[s] ${FRAMEWORK_ROOT}/blueprint-rs/crates
COPY . .
# No database at build time: sqlx macros check queries offline.
ENV SQLX_OFFLINE=true
RUN cargo build --release --workspace --bins

{{{runtime_stage}}}
COPY --from=builder /app/target/release /app/target/release
# Deployed PostgreSQL URLs verify the server against the AWS RDS CA bundle.
ADD --chmod=0644 https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem /app/certs/rds-global-bundle.pem

ENV HOST=0.0.0.0
ENV PROTOCOL=http
ENV PORT=8000
ENV VERSION=v1
ENV DOCS_PATH=/docs

EXPOSE 8000
