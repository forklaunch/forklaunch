---
name: rust-service-recipes
description: "Add a real feature to a Rust ForkLaunch service: schemas, protected endpoints, per-user storage, migration, tests. Worked example with code that compiles."
user-invokable: true
---

# Rust service recipes

Use this when a Rust service scaffolded by `forklaunch init service <name> --language rust -d postgresql`
needs real endpoints and storage. Everything here was compiled and run against
`forklaunch-core`/`forklaunch-axum` 0.2.5 with PostgreSQL. Copy the shapes and rename; do not
read the framework source or build its docs to rediscover them. `/rust-framework` has the
policies behind these shapes.

The example adds "teas" to a service called `records`: a signed-in person adds a tea, lists
their teas, and marks one as a favourite. Paths below are relative to the service folder
(`src/modules/records`).

## The order that works

1. `forklaunch init router teas --language rust --path src/modules/records`. This creates
   `api/controllers/teas.rs`, `api/routes/teas.rs` and `domain/schemas/teas.rs`, and registers
   them in `api/controllers/mod.rs`, `api/routes/mod.rs`, `domain/schemas/mod.rs` and
   `.forklaunch/contracts/lib.rs`. Never hand-edit those four registration files.
2. Replace the three generated files with your schemas, contract + handlers, and route bindings.
3. Add a migration, a stored entity, and repository methods.
4. `cargo build -p records` (from the app root). The build regenerates the OpenAPI document and
   both SDKs under `sdk/`. Keep every generated file under `sdk/` and save it with the code:
   publishing uses them.
5. `make lint` and `make test` in the service folder.

A first `cargo build` of a new app takes several minutes; later builds take seconds. Run
`cargo check -p records` while iterating.

## 1. Schemas: `domain/schemas/teas.rs`

These structs are the API contract. `#[forklaunch_core::contract::schema]` supplies
serialization and the JSON schema; do not add `serde` or `schemars` derives yourself.

```rust
//! What the teas API accepts and returns. These structs are the contract: the
//! OpenAPI document and both SDKs are generated from them.

#[forklaunch_core::contract::schema(rename_all = "camelCase")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tea {
    pub id: String,
    pub name: String,
    pub note: String,
    pub favourite: bool,
    pub created_at: String,
}

#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTea {
    #[field(min_length = 1, max_length = 80)]
    pub name: String,
    #[field(max_length = 500)]
    pub note: String,
}

#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeaList {
    pub teas: Vec<Tea>,
}

#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavouriteChange {
    pub favourite: bool,
}

/// The `:id` in `/teas/:id/favourite`.
#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeaPath {
    pub id: String,
}
```

- Field limits: `#[field(min_length = 1, max_length = 80)]` on strings, `#[field(min = 0, max = 100)]`
  on numbers, `#[field(optional)]` for a field that may be left out. Requests that break a limit
  are rejected with 400 before your handler runs.
- `usize`/`u64` fields need `#[field(max = 9007199254740991)]` so the TypeScript SDK can hold them.
- A path parameter is a struct with one `String` field per `:name` in the path.

## 2. Contract and handlers: `api/controllers/teas.rs`

```rust
use crate::api::policy::UserPolicy;
use crate::domain::schemas::teas::{FavouriteChange, NewTea, Tea, TeaList, TeaPath};
use crate::error::AppError;
use crate::registrations::AppState;
use forklaunch_axum::native::{EndpointError, State};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_core::contract::endpoint::{NoFields, RequestFields};
use uuid::Uuid;

/// Internal errors never reach the caller as text; each maps to a fixed status.
fn failed(error: AppError) -> EndpointError {
    match error {
        AppError::NotFound(_) => EndpointError::NotFound,
        _ => EndpointError::Unavailable,
    }
}

pub async fn list_teas(
    State(state): State<AppState>,
    session: AuthenticatedSession,
    (): (),
) -> Result<TeaList, EndpointError> {
    let teas = state.repository.teas(&session.principal.subject).await.map_err(failed)?;
    Ok(TeaList { teas })
}

pub async fn add_tea(
    State(state): State<AppState>,
    session: AuthenticatedSession,
    new: NewTea,
) -> Result<Tea, EndpointError> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err(EndpointError::InvalidRequest);
    }
    state
        .repository
        .add_tea(&session.principal.subject, name, new.note.trim())
        .await
        .map_err(failed)
}

pub async fn set_favourite(
    State(state): State<AppState>,
    session: AuthenticatedSession,
    request: RequestFields<FavouriteChange, NoFields, NoFields, TeaPath>,
) -> Result<Tea, EndpointError> {
    let id = Uuid::parse_str(&request.path.id).map_err(|_| EndpointError::NotFound)?;
    state
        .repository
        .set_favourite(&session.principal.subject, id, request.body.favourite)
        .await
        .map_err(failed)
}

pub fn policy() -> UserPolicy {
    UserPolicy
}

#[forklaunch_core::contract::api]
pub mod contract {
    use crate::domain::schemas::teas::{FavouriteChange, NewTea, Tea, TeaList, TeaPath};
    use forklaunch_core::contract::{
        HttpMethod,
        endpoint::{Endpoint, Fields, JsonBody, NoBody, NoFields},
    };

    #[access(Protected)]
    pub fn list_teas() -> Endpoint<NoBody, JsonBody<TeaList>> {
        Endpoint::new(
            "List Teas",
            "Lists the signed-in person's teas, newest first",
            HttpMethod::Get,
            "/teas",
            200,
        )
        .expect("valid list endpoint")
    }

    #[access(Protected)]
    pub fn add_tea() -> Endpoint<JsonBody<NewTea>, JsonBody<Tea>> {
        Endpoint::new(
            "Add Tea",
            "Adds a tea for the signed-in person",
            HttpMethod::Post,
            "/teas",
            201,
        )
        .expect("valid add endpoint")
    }

    #[access(Protected)]
    pub fn set_favourite()
    -> Endpoint<Fields<JsonBody<FavouriteChange>, NoFields, NoFields, TeaPath>, JsonBody<Tea>> {
        Endpoint::new(
            "Set Favourite",
            "Marks or unmarks one of the signed-in person's teas as a favourite",
            HttpMethod::Put,
            "/teas/:id/favourite",
            200,
        )
        .expect("valid favourite endpoint")
    }
}
```

Rules the compiler enforces (each costs a build cycle if you miss it):

- Each `pub fn` in `mod contract` becomes an operation type in PascalCase: `set_favourite` →
  `contract::SetFavourite`. **A schema struct must not have the same name as an operation**
  (that is why the body is `FavouriteChange`, not `SetFavourite`).
- `mod contract` is also compiled alone by the private contracts crate. Inside it, refer to
  schemas only as `crate::domain::schemas::...`, and import nothing else from the service.
- Bodies: `NoBody`, `JsonBody<T>`. For path parameters wrap the body:
  `Fields<JsonBody<Body>, NoFields, NoFields, PathStruct>` (use `Fields<NoBody, NoFields, NoFields, PathStruct>`
  when there is no body). The handler then receives `RequestFields<Body, NoFields, NoFields, PathStruct>`
  with `.path` and `.body`. Import these from `forklaunch_core::contract::endpoint`
  (`endpoint::fields` is private).
- `#[access(Protected)]` requires a signed-in person. A protected handler is
  `async fn(State<AppState>, AuthenticatedSession, Input) -> Result<Output, EndpointError>`.
  A `NoBody` input arrives as `()`. Without the attribute the endpoint is public and the handler
  takes only `State<AppState>` (and the input, if any).
- The signed-in person's id is `session.principal.subject`. Never take a user id from the
  request body or path.
- The status code is fixed in `Endpoint::new(..., 201)`. A handler returns the value or one of
  `EndpointError::{InvalidRequest, Unauthorized, Forbidden, NotFound, Conflict, Unavailable}`;
  it cannot send custom error text.

## 3. Route bindings: `api/routes/teas.rs`

```rust
use crate::api::controllers::teas::{add_tea, contract, list_teas, set_favourite};
use crate::{api::policy::UserPolicy, registrations::AppState};
use forklaunch_axum::native::ContractRouter;

pub fn mount(router: ContractRouter<AppState>) -> Result<ContractRouter<AppState>, String> {
    contract::Bindings {
        list_teas: contract::ListTeas::bind_authorized(UserPolicy, list_teas),
        add_tea: contract::AddTea::bind_authorized(UserPolicy, add_tea),
        set_favourite: contract::SetFavourite::bind_authorized(UserPolicy, set_favourite),
    }
    .mount(router)
}
```

`Bindings` has one field per operation; leaving one out is a compile error. Public operations
use `contract::Name::bind(handler)`; protected ones use `bind_authorized(UserPolicy, handler)`.
`UserPolicy` already exists in `api/policy.rs`.

## 4. Migration: `migrations/0002_teas.sql`

```sql
-- One row per tea. Every table carries tenant_id with forced row-level
-- security; owner_id is the signed-in person the row belongs to.
CREATE TABLE teas (
    tenant_id text NOT NULL,
    id uuid NOT NULL,
    owner_id text NOT NULL,
    name_ciphertext text NOT NULL,
    note_ciphertext text NOT NULL,
    favourite boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX teas_owner_newest ON teas (tenant_id, owner_id, created_at DESC, id DESC);
ALTER TABLE teas ENABLE ROW LEVEL SECURITY;
ALTER TABLE teas FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON teas
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));
```

Add a new numbered file; never edit a migration that has been saved. Every table has `tenant_id`,
the row-level-security block above, and `PRIMARY KEY (tenant_id, id)`. Data that belongs to one
person also has `owner_id`. Migrations run with `cargo run -p records -- --migrate`
(`MIGRATION_DATABASE_URL`), never at startup.

## 5. Stored entity: `persistence/entities/stored_tea.rs`

```rust
//! A tea as it is stored. Text a person typed is personal data: it is held as
//! a `ProtectedField` (encrypted), never as a plain `String`.
use forklaunch_core::common::FrameworkResult;
use forklaunch_core::persistence::{
    compliance::ComplianceMetadata,
    protected::{ProtectedField, storage::ProtectedModel},
};
use uuid::Uuid;

#[derive(Debug, ComplianceMetadata)]
#[compliance(entity = "Tea", persisted)]
pub struct StoredTea {
    #[compliance(level = "none")]
    pub id: Uuid,
    #[compliance(level = "none")]
    pub tenant_id: String,
    #[compliance(level = "none")]
    pub owner_id: String,
    #[compliance(level = "pii", purpose = "tea-name-v1")]
    pub name_ciphertext: ProtectedField,
    #[compliance(level = "pii", purpose = "tea-note-v1")]
    pub note_ciphertext: ProtectedField,
    #[compliance(level = "none")]
    pub favourite: bool,
}
impl StoredTea {
    pub fn validate(&self) -> FrameworkResult<()> {
        self.validate_protected_storage()
    }
}
```

Every field declares `#[compliance(level = ...)]`. Text a person typed is `level = "pii"` with a
unique `purpose`, typed `ProtectedField`, in a `..._ciphertext` column of type `text`; a plain
`String` there does not compile. Ids, flags, counts and timestamps are `level = "none"`.

Register the entity in `persistence/compliance_schema.rs` and re-export it from
`persistence/entities/mod.rs`:

```rust
//! The exporter compiles the same protected domain model used by the repository.
#[allow(dead_code)]
#[path = "entities/stored_record.rs"]
mod stored_record;
pub use stored_record::StoredRecord;
#[allow(dead_code)]
#[path = "entities/stored_tea.rs"]
mod stored_tea;
use forklaunch_core::persistence::compliance::ComplianceEntity;
pub use stored_tea::StoredTea;
pub fn declarations() -> Vec<ComplianceEntity> {
    vec![
        ComplianceEntity::of::<StoredRecord>("records", "postgres-table"),
        ComplianceEntity::of::<StoredTea>("teas", "postgres-table"),
    ]
}
```

## 6. Repository methods: `persistence/teas.rs`

Add `mod teas;` to `persistence/mod.rs`. In `persistence/repository.rs` make the four
`PostgresRepository` fields and `fn unavailable()` `pub(super)` so this file can use them.

```rust
//! Tea storage. Every query runs inside a tenant transaction and filters by
//! both tenant and owner, so one person can never read or change another's rows.
use super::repository::{PostgresRepository, unavailable};
use crate::domain::schemas::teas::Tea;
use crate::error::{AppError, Result};
use crate::persistence::compliance_schema::StoredTea;
use forklaunch_core::common::FrameworkResult;
use forklaunch_core::persistence::{
    begin_tenant_transaction,
    protected::{
        ComplianceAccess, ComplianceAudit, ProtectedContext, ProtectedField, ProtectedMember,
    },
};
use uuid::Uuid;

/// Records each time protected text is decrypted for its owner. Service logs
/// are collected by the platform; no plaintext is ever written here.
struct AccessLog;
impl ComplianceAudit for AccessLog {
    fn record(&self, event: &ComplianceAccess<'_>) -> FrameworkResult<()> {
        eprintln!(
            "protected-access entity={} field={} resource={} actor={} reason={}",
            event.entity, event.field, event.resource, event.actor, event.reason
        );
        Ok(())
    }
}

type Row = (Uuid, String, String, bool, String);
// sqlx only accepts literal SQL, so the column list is spelled out in each query.
// created_at is read as text: this service's sqlx has no date/time feature.

impl PostgresRepository {
    fn seal(
        &self,
        id: Uuid,
        member: FrameworkResult<ProtectedMember>,
        text: &str,
    ) -> Result<ProtectedField> {
        let context = ProtectedContext::new(self.tenant, id).map_err(|_| unavailable())?;
        ProtectedField::seal(
            &self.key,
            context,
            member.map_err(|_| unavailable())?,
            text.as_bytes(),
        )
        .map_err(|_| unavailable())
    }

    fn reveal(
        &self,
        id: Uuid,
        member: FrameworkResult<ProtectedMember>,
        ciphertext: String,
        owner: &str,
    ) -> Result<String> {
        let context = ProtectedContext::new(self.tenant, id).map_err(|_| unavailable())?;
        let field = ProtectedField::from_ciphertext(member.map_err(|_| unavailable())?, ciphertext)
            .map_err(|_| unavailable())?;
        let plaintext = field
            .reveal(&self.key, context, owner, "owner reads own tea", &AccessLog)
            .map_err(|_| unavailable())?;
        String::from_utf8(plaintext.to_vec()).map_err(|_| unavailable())
    }

    fn tea(&self, owner: &str, (id, name, note, favourite, created_at): Row) -> Result<Tea> {
        Ok(Tea {
            id: id.to_string(),
            name: self.reveal(id, StoredTea::protected_name_ciphertext(), name, owner)?,
            note: self.reveal(id, StoredTea::protected_note_ciphertext(), note, owner)?,
            favourite,
            created_at,
        })
    }

    pub async fn teas(&self, owner: &str) -> Result<Vec<Tea>> {
        let mut transaction =
            begin_tenant_transaction(&self.pool, &self.context).await.map_err(|_| unavailable())?;
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id,name_ciphertext,note_ciphertext,favourite,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') FROM teas WHERE tenant_id=$1 AND owner_id=$2 \
             ORDER BY created_at DESC, id DESC LIMIT 200",
        )
        .bind(self.context.id())
        .bind(owner)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())?;
        rows.into_iter().map(|row| self.tea(owner, row)).collect()
    }

    pub async fn add_tea(&self, owner: &str, name: &str, note: &str) -> Result<Tea> {
        let id = Uuid::new_v4();
        let row = StoredTea {
            id,
            tenant_id: self.context.id().to_owned(),
            owner_id: owner.to_owned(),
            name_ciphertext: self.seal(id, StoredTea::protected_name_ciphertext(), name)?,
            note_ciphertext: self.seal(id, StoredTea::protected_note_ciphertext(), note)?,
            favourite: false,
        };
        row.validate().map_err(|_| unavailable())?;
        let mut transaction =
            begin_tenant_transaction(&self.pool, &self.context).await.map_err(|_| unavailable())?;
        let stored: Row = sqlx::query_as(
            "INSERT INTO teas (tenant_id,id,owner_id,name_ciphertext,note_ciphertext,favourite) \
             VALUES ($1,$2,$3,$4,$5,$6) \
             RETURNING id,name_ciphertext,note_ciphertext,favourite,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
        )
        .bind(&row.tenant_id)
        .bind(row.id)
        .bind(&row.owner_id)
        .bind(row.name_ciphertext.ciphertext())
        .bind(row.note_ciphertext.ciphertext())
        .bind(row.favourite)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())?;
        self.tea(owner, stored)
    }

    /// Setting a state (not toggling) keeps a repeated request harmless.
    pub async fn set_favourite(&self, owner: &str, id: Uuid, favourite: bool) -> Result<Tea> {
        let mut transaction =
            begin_tenant_transaction(&self.pool, &self.context).await.map_err(|_| unavailable())?;
        let stored: Option<Row> = sqlx::query_as(
            "UPDATE teas SET favourite=$4 WHERE tenant_id=$1 AND owner_id=$2 AND id=$3 \
             RETURNING id,name_ciphertext,note_ciphertext,favourite,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
        )
        .bind(self.context.id())
        .bind(owner)
        .bind(id)
        .bind(favourite)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())?;
        // Someone else's tea and a missing tea look the same to the caller.
        let stored = stored.ok_or_else(|| AppError::NotFound("tea".into()))?;
        self.tea(owner, stored)
    }
}
```

- Every query: `begin_tenant_transaction`, then `WHERE tenant_id=$1` with `self.context.id()`,
  plus `AND owner_id=$2` for a person's own data, then `commit`.
- sqlx 0.9 accepts only literal SQL strings. `format!` and string constants built at run time do
  not compile; write the column list out in each query and pass values with `.bind`.
- This service's sqlx has no date/time feature. Read timestamps as text with `to_char` as above.
- Encrypt with `seal` before writing; decrypt with `reveal`, which needs who is reading, why,
  and an audit sink. Use the same id for the row and its `ProtectedContext`.
- Return "not found" for a row that exists but belongs to someone else.
- To set a value, write the value (`favourite=$4`), not a toggle, so repeating a request is harmless.

## 7. Test: `__test__/teas.rs`

Add to `Cargo.toml`:

```toml
[[test]]
name = "teas"
path = "__test__/teas.rs"
```

```rust
use records::error::AppError;
use records::persistence::{PostgresRepository, database::DatabaseConfig};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires migrated PostgreSQL, restricted DATABASE_URL, TENANT_ID and encryption keys"]
async fn a_person_only_sees_and_changes_their_own_teas() {
    let repository =
        PostgresRepository::connect(DatabaseConfig::from_env().unwrap()).await.unwrap();
    let (ana, ben) = (format!("user-{}", Uuid::new_v4()), format!("user-{}", Uuid::new_v4()));

    assert!(repository.teas(&ana).await.unwrap().is_empty());
    let first = repository.add_tea(&ana, "Sencha", "Grassy, 70C").await.unwrap();
    let second = repository.add_tea(&ana, "Assam", "").await.unwrap();
    assert!(!first.favourite);

    let listed = repository.teas(&ana).await.unwrap();
    assert_eq!(listed.iter().map(|tea| tea.name.as_str()).collect::<Vec<_>>(), ["Assam", "Sencha"]);
    assert_eq!(listed[1].note, "Grassy, 70C");
    assert!(repository.teas(&ben).await.unwrap().is_empty());

    let id = Uuid::parse_str(&first.id).unwrap();
    for _ in 0..2 {
        assert!(repository.set_favourite(&ana, id, true).await.unwrap().favourite);
    }
    assert!(matches!(repository.set_favourite(&ben, id, true).await, Err(AppError::NotFound(_))));
    assert!(matches!(
        repository.set_favourite(&ana, Uuid::new_v4(), true).await,
        Err(AppError::NotFound(_))
    ));
    assert_eq!(repository.teas(&ana).await.unwrap().iter().filter(|tea| tea.favourite).count(), 1);
    let _ = second;
    repository.close().await;
}
```

Database tests are `#[ignore]` so `cargo test` passes without a database. Run them with
`cargo test -p records --test teas -- --ignored` after migrating a database and setting
`DATABASE_URL` (the restricted app role), `TENANT_ID`, `STORAGE_ENCRYPTION_NAMESPACE`,
`ENCRYPTION_KEY_HEX`, `ENCRYPTION_KEY_ID` and `DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT=true`.

## Sign-in between the web app and the service

Protected endpoints accept `Authorization: Bearer <session JWT>` issued by the IAM service
(Better Auth's JWT plugin) and verify it against `JWKS_PUBLIC_KEY_URL`. Give the Rust service
the same value the IAM service uses, for example `http://iam:8000/api/auth/jwks` in
`docker-compose.yaml`, plus `JWT_ISSUER` and `JWT_AUDIENCE` if IAM sets them. Without
`JWKS_PUBLIC_KEY_URL` the service starts but every protected endpoint answers 401. List the web
app's origin in `CORS_ORIGINS`.

The web app calls the service through the generated TypeScript SDK in `sdk/typescript`
(rebuilt by `cargo build`), sending the session JWT it gets from IAM. Do not hand-write fetch
calls or copy types.

## What a wrong turn looks like

| Symptom | Cause |
| --- | --- |
| `generates the operation type X ... collides with X` | A schema struct shares an operation's name. Rename the struct. |
| `module fields is private` | Import `Fields`, `NoFields`, `RequestFields` from `contract::endpoint`. |
| `dynamic SQL strings should be audited` | SQL built with `format!`. Use a literal string and `.bind`. |
| `cannot bind an #[access(Protected)] operation` | Used `bind` instead of `bind_authorized(UserPolicy, ...)`. |
| `missing field` in `Bindings` | An operation in `mod contract` has no binding in `api/routes`. |
| 401 on every protected call | `JWKS_PUBLIC_KEY_URL` is not set for the service. |
| Startup error naming an environment variable | Every setting in `config.rs` is required; see the service's block in `docker-compose.yaml`. |
