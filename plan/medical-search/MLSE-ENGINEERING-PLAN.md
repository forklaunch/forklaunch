# MLSE — Engineering Implementation Plan

| | |
|---|---|
| **Version / Date** | 2.0 · 24 September 2026 |
| **Prepared by** | ForkLaunch Engineering |
| **Status** | Ready to start. Implementation Plan approved by the founder |
| **Delivery** | One branch (`feat/mlse-module`, from latest `main`) and **one pull request**, built in phases as reviewable commits |
| **Parent documents** | MLSE Implementation Plan, MLSE Team Brief, MLSE Design Document |

MLSE is a new first-class ForkLaunch module, installed the same way as `iam-base`: `forklaunch init module -m mlse-base`. This plan lists everything the repository needs, found by tracing every place an existing module is wired in, and the order to build it in. Paths are relative to the repository root.

---

## 1. Repository analysis: what MLSE needs

### 1.1 Reused as-is

| Capability | Where it lives |
|---|---|
| HTTP framework, DI, OpenAPI and typed SDK | `framework/core`, `framework/express` |
| Entities with data classification and encryption | `defineComplianceEntity`, `EncryptedType`, `FieldEncryptor` (`framework/core/src/persistence/`) |
| Multi-tenancy, erasure/export, retention | `tenantFilter.ts`, `ComplianceDataService`, `RetentionService` |
| Authentication and roles | IAM module; `surfacing.ts` pattern with Redis-backed `AuthCacheService` |
| Cache | `@forklaunch/infrastructure-redis` (`RedisTtlCache`) |
| Background jobs | `@forklaunch/implementation-worker-redis` (same backend `ecommerce-stripe` uses) |
| Test harness | `@forklaunch/testing` (`BlueprintTestHarness`, testcontainers) |
| Monitoring | OpenTelemetry collector and metrics definitions |

`framework/` needs **no changes** for this PR (see 1.5 for the one follow-up).

### 1.2 CLI wiring: every file that registers a module

Traced from all existing module references. Each item is a mechanical addition next to the existing `cac`/`messaging` entries.

| File | What to add for MLSE |
|---|---|
| `cli/src/constants.rs` | `Module::BaseMlse` choice (`id: "mlse-base"`, `exclusive_files: Some(&["mlse-base"])`); `get_service_module_name` → `"mlse"`; `get_service_module_description` → `"medical literature search service APIs"`; `get_service_module_cache` arm |
| `cli/src/core/modules.rs` | `MlseConfig { BaseMlse }`, `ModuleConfig.mlse`, `validate_modules()` arm |
| `cli/src/core/manifest/service.rs` | `is_mlse: bool` |
| `cli/src/init/module.rs` | `is_mlse`; add `BaseMlse` to `is_cache_enabled` (Redis for live-retrieval cache and worker queue) and to `ships_worker` (ingestion worker) |
| `cli/src/init/application.rs` | `mlse: None` in both default module configs (~lines 630, 643); module sort order (`BaseMlse => 5`, `Relay` moves to 6); `is_mlse`, `ships_worker` and `is_cache_enabled` for the template dir; `service_data.is_mlse` from global config |
| `cli/src/init/service.rs` | Add `is_mlse` to the `main`/`types` override conditions (~lines 335, 340); dependency fields for the two MLSE packages (~line 535); `is_mlse: false` for plain services |
| `cli/src/init/worker.rs` | `forklaunch_interfaces_mlse: None` |
| `cli/src/init/relay.rs` | `is_mlse: false` |
| `cli/src/core/template.rs` | Router registry arm (`Module::BaseMlse => None` until routers exist, then the router list) |
| `cli/src/core/client_sdk.rs` | `is_mlse_enabled` parameter adding the `@{app}/mlse` workspace dependency |
| `cli/src/templates/project/client-sdk/clientSdk.ts` | `MlseSdkClient` import and `mlseSdkClient` export, gated by `{{#is_mlse}}` |
| `cli/src/core/bunfig.rs` | `@forklaunch/implementation-mlse-base`, `@forklaunch/interfaces-mlse` in the package list |
| `cli/src/core/package_json/package_json_constants.rs` | Version constants for both packages, in the format `check_blueprint_deps` validates |
| `cli/src/core/package_json/project_package_json.rs` | `ProjectDependencies` fields, serializer entries and deserializer match arms for both packages |
| `cli/src/core/docker.rs` | pgvector image handling (1.4) |
| `cli/src/templates/project/mlse-base/*` | Git-symlink farm into `blueprint/mlse-base/*` (`.env.local`, `.env.test`, `.gitignore`, `api`, `bootstrapper.ts`, `index.ts`, `mikro-orm.config.ts`, `persistence`, `registrations.ts`, `sdk.ts`, `server.ts`, `worker.ts`, `migrations`, `scripts`, …) |

### 1.3 Blueprint, CI and documentation

| File | Change |
|---|---|
| `blueprint/pnpm-workspace.yaml` | Add `mlse-base`, `interfaces/mlse`, `implementations/mlse/base` |
| `blueprint/.changeset/config.json` | Add `@forklaunch/blueprint-mlse-base` to `ignore` (the app package is not published) |
| `blueprint/.changeset/mlse-module.md` | Changeset for `@forklaunch/interfaces-mlse` and `@forklaunch/implementation-mlse-base` |
| `blueprint/docker-compose.base.yml` | PostgreSQL image → pgvector (1.4) |
| `cli/tests/init_mlse.sh` (new, executable) | Scaffold an app with `mlse-base`, build, type-check |
| `cli/tests/init_module.sh` | Add `mlse-base` to the node and bun multi-module scaffolds |
| `.github/workflows/e2e_tests.yml` | Add `init_mlse.sh` to group 6, or it never runs in CI |
| `docs/adding-projects/modules.md` | Module table row and `--module` option list |
| `cli/assets/forklaunch-skills/cli/SKILL.md` | Add `mlse-base` to the module list (~line 340) |

### 1.4 Database: pgvector (findings verified with Docker)

- `postgres:latest` currently resolves to **PostgreSQL 18** (Debian 13 base).
- **`pgvector/pgvector:pg18-trixie`** matches both the major version and the Debian base; pgvector **0.8.6** was verified working in it (`CREATE EXTENSION vector` and a distance query). Use the `-trixie` tag: the plain `pg18` tag uses Debian 12, and switching an existing database between OS versions risks collation mismatches.
- The CLI adds **one shared PostgreSQL service per application**, and only if none exists (`add_database_to_docker_compose`, `cli/src/core/docker.rs` ~line 1350). Therefore:
  - New application with MLSE: create the PostgreSQL service with the pgvector image.
  - MLSE added to an application that already has `postgres:*`: **replace that service's image** with the pgvector image. Other services are unaffected, since the pgvector image is the official PostgreSQL image plus the extension.
  - Application without MLSE: unchanged (`postgres:latest`).
- Pin the tag (`pg18-trixie`), and record the PostgreSQL major in a constant so future upgrades change one line.

### 1.5 Testing with pgvector

`BlueprintTestHarness` hardcodes `postgres:latest` (`framework/testing/src/containers.ts:145`). Changing it would require publishing `@forklaunch/testing` before MLSE could use it, which would break this PR's CI. So:
- **In this PR:** `blueprint/mlse-base/__test__/pgvector-container.ts` starts `pgvector/pgvector:pg18-trixie` with `testcontainers` (already the harness's dependency) and wires it into the harness's ORM setup.
- **Follow-up (separate, small):** add an optional `image` to the harness's PostgreSQL config, publish, then switch MLSE to it.

### 1.6 Other findings

| Finding | Consequence |
|---|---|
| MikroORM is 7.2.1 across blueprints | The embedding column uses a custom MikroORM `Type` (precedent: `EncryptedType`) mapping `number[]` to `vector(n)`, unless MikroORM 7 provides a native vector type; check first |
| `ecommerce-stripe` is the precedent for a module that ships a worker | Copy its `worker.ts`, `dev:worker`/`start:worker` scripts, and Redis worker registration; the CLI then adds a worker container to docker compose automatically (`ships_worker`) |
| Blueprint dependency CI (`blueprint_depcheck.yml`) runs `forklaunch depcheck` and `check_blueprint_deps` | Keep `package_json_constants.rs` in sync with the new `package.json` versions, or CI fails |
| Rust is not installed on the current dev machine; Docker 29.1.3 is | Install Rust (`rustup`) to build and test the CLI locally; Docker covers database tests |
| `core.symlinks=false` on this Windows checkout | Template symlinks appear as text files locally; they are correct in git and on Linux CI |

---

## 2. Module design

### 2.1 Packages

```
blueprint/interfaces/mlse/             ContentSourceProvider, LlmProvider, SearchService,
                                       AnswerService contracts; shared types and enums
blueprint/implementations/mlse/base/   Pure logic: LicenseGate, QueryClassifier, CitationVerifier,
                                       NumberVerifier, FrameworkAssembler, PublicCorpusProvider,
                                       source fetchers, FakeLlmProvider; unit tests; README
blueprint/mlse-base/                   The service: entities, migrations, controllers, routes,
                                       SearchService, LiveRetrievalService, AnswerService,
                                       IngestionService, worker.ts, scripts, e2e tests
```

### 2.2 Configuration (per client)

| Variable | Purpose |
|---|---|
| `DB_*`, `REDIS_URL` | Standard, scaffolded by the CLI |
| `NCBI_API_KEY` (optional), `NCBI_TOOL`, `NCBI_EMAIL` | PubMed/PMC E-utilities (NCBI asks apps to let users supply their own key and send `tool`/`email`) |
| `OPENFDA_API_KEY` (optional) | Higher openFDA rate limit |
| `LLM_PROVIDER`, `LLM_API_KEY`, `LLM_MODEL` | Answer drafting |
| `EMBEDDING_MODEL`, `EMBEDDING_DIMENSIONS` | Embeddings; the dimension fixes the vector column size |
| `LIVE_RETRIEVAL_TIMEOUT_MS` | Budget for live source queries within the 10-second target |

### 2.3 API surface

| Endpoint | Permission |
|---|---|
| `GET /search` | `clinician:search` |
| `POST /answer` (streams verified sections) | `clinician:search` |
| `GET /entities/:type/:id`, `GET /entities/procedure/:id/phases/:n` | `clinician:search` |
| `GET /source`, `GET /document/:id` | `clinician:search` |
| `GET/POST /saved-search` | `clinician:save_search` |
| `/admin/framework/*`, `/admin/content-flag/*` | `reviewer:manage_content` |
| `/admin/source/*`, `/admin/content-license/*` | `admin:manage_sources` |
| Compliance erase/export | Internal (HMAC), generated as for every service |

Every endpoint is added to `sdk.ts` in the same commit as its route.

---

## 3. Progress

| Phase | Status | Verified |
|---|---|---|
| A. Plan documents | Done | — |
| B. CLI registration | Done | 764 cargo tests pass (Linux container with real symlinks and LF endings) |
| C. pgvector | Done | Unit tests; scaffold of a new app uses `pg18-trixie`; adding MLSE to an existing app upgrades `postgres:latest`; adding another module afterwards keeps pgvector; MySQL with MLSE is rejected |
| D. Package skeletons | Done | Node 24 / pnpm 11.1.0 build; 32 unit tests; migration e2e test on pgvector (extension, distance query, `source` table); `forklaunch depcheck` and `check_blueprint_deps` pass |
| E–J | Not started | — |

**Before merge:** `@forklaunch/interfaces-mlse` and `@forklaunch/implementation-mlse-base` must be published to npm (0.1.x), because the CLI scaffold tests (`init_mlse.sh`, `init_module.sh`) run `pnpm install` against the registry.

## 4. Build phases (commits within the one PR)

| Phase | Scope | Done when |
|---|---|---|
| **A. Plan documents** | Bring the approved plan documents onto `feat/mlse-module` | Docs present on the branch |
| **B. CLI registration** | Every item in 1.2 except pgvector; symlink farm; `init_mlse.sh`, `init_module.sh`, e2e workflow, docs, skill list | `cargo test` passes, including `every_module_variant_has_an_embedded_template_dir`; `init_mlse.sh` scaffolds and type-checks |
| **C. pgvector** | `docker.rs` new/existing/unchanged cases; `docker-compose.base.yml`; image constant | Three Rust unit tests pass; scaffolded compose uses `pg18-trixie` |
| **D. Package skeletons** | Three packages with config symlinks, `package.json`s, workspace and changeset entries, `bootstrapper.ts`, `registrations.ts` (ORM, OTel, compliance, retention, Redis cache, worker, providers), `server.ts`, `worker.ts`, `sdk.ts`; migration 0 (`CREATE EXTENSION vector`); `GET /source`; pgvector test container helper | Service boots; e2e test confirms the `vector` extension exists; depcheck CI passes |
| **E. Corpus and ingestion** | Entities (`Source`, `Document`, `DocumentVersion`, `DocumentChunk` with `tsvector` + vector, `MedicalConcept`), migration 1 with GIN and HNSW indexes; `LicenseGate`; fetchers for openFDA, DailyMed, ClinicalTrials.gov, MeSH, PubMed, PMC OA; `IngestionService` and worker; `refresh-corpus.ts` | Fixture-based ingestion tests; `metadata_only` documents never store full text; label re-ingest creates a version |
| **F. Search and live retrieval** | Embeddings via `LlmProvider.embed`; `SearchService` (MeSH expansion, full-text + vector, fusion, re-rank interface); `LiveRetrievalService` (E-utilities, openFDA, ClinicalTrials.gov; license gate; Redis cache; per-source rate limits); `GET /search`; `run-eval.ts` | Recorded-fixture tests (no network); rate limits respected; evaluation baseline recorded |
| **G. Topic pages and case studies** | `Topic`, `QuestionFramework`, `ProcedurePhase`, `ProcedureStep`, `QuantitativeFact`, `Evidence`, `CaseReport`; framework seeds; `FrameworkAssembler`; case-report detection, relevance filter and extraction; entity endpoints | Every field references evidence; irrelevant cases excluded; case evidence labelled |
| **H. Answer generation and verification** | `QueryClassifier` and safety paths; `AnswerService` with self-check; `CitationVerifier`; `NumberVerifier`; `GeneratedAnswer`/`Citation` audit; `POST /answer` with streaming | Full safety and verification test list passes; evaluation targets met on launch topics |
| **I. Licensed content, governance, voice readiness** | `LicensedContentProvider` adapter, `ContentLicense`, `ContentSourceResolver` (fail closed); admin endpoints; `spoken_summary`; transcript identifier removal; saved searches; history retention; metrics | Tenant isolation and fail-closed tests pass; retention job anonymizes history |
| **J. Hardening** | README for client developers; security review; permissions seed guidance; final CI run on a freshly scaffolded project | PR ready for review |

---

## 5. Test plan

| Level | Scope | Tooling |
|---|---|---|
| Rust unit | Module registration, pgvector compose cases, package-json round-trip | `cargo test` in `cli/` |
| CLI scaffold | `init_mlse.sh`, `init_module.sh` | Shell tests in CI group 6 and 5 |
| TypeScript unit | License gate, classifier, verifiers, assembler, fetcher parsing | Vitest in `implementations/mlse/base` |
| End-to-end | Migrations, ingestion, search, answers, auth, tenant isolation, safety paths | Vitest + harness + pgvector container, real Redis, fake `LlmProvider`, recorded source fixtures |
| Evaluation | Retrieval and answer quality on launch topics | `scripts/run-eval.ts` with the clinician gold set |

No test calls a live AI provider or a live medical API.

---

## 6. Decisions needed during the build

| # | Decision | Needed by |
|---|---|---|
| T1 | Confirm module name `mlse` | Phase B |
| T2 | Embedding model and dimension | Phase E |
| T3 | Production AI provider and data-handling terms | Phase H |
| T4 | Launch topics (for fixtures, frameworks and the evaluation set) | Phase G |
| T5 | Install Rust on the dev machine, or build the CLI in a Docker Rust container | Phase B |

---

## 7. Definition of done

- One pull request from `feat/mlse-module` to `main`, all phases as separate commits.
- CI green: Rust tests, CLI e2e group 6 including `init_mlse.sh`, blueprint depcheck, blueprint unit and e2e tests.
- A freshly scaffolded application with `mlse-base` builds, starts with pgvector, and passes health and search checks.
- Every endpoint in `sdk.ts`; permissions documented; README complete.
