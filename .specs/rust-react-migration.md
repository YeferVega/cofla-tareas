# rust-react-migration

## 1. Goal & Scope

Migrate `cofla-tareas` from the legacy Node.js `server.js` + vanilla `public/` frontend
to a **Rust backend** (`backend/`) and a **React + TypeScript frontend** (`frontend/`),
preserving the legacy behavior exactly as a functional baseline:

- API contracts: `GET /health`; `GET|POST /api/tasks`; `PATCH|DELETE /api/tasks/:id`;
  task shape `{ id, title, done }`; health shape `{ ok: true, storage, version }`;
  delete success shape `{ ok: true }`.
- Status semantics: `400` invalid input/ID/JSON, `404` unknown route **and wrong
  method** (no `405` anywhere), `413` body > 8192 bytes, `415` content type not
  starting with `application/json`, `503` for all unknown failures; every error body
  is `{ "error": "<message>" }`.
- Validation: task `id` is a safe integer in `1..=2147483647`; `title` is 1–200
  characters after trimming; PATCH `done` must be a boolean.
- Security headers on **every** response:
  `Content-Security-Policy: default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'`,
  `X-Content-Type-Options: nosniff`, and `Cache-Control: no-store` on JSON responses.
- Dual storage behind `STORAGE=memory` (volatile, in-process) | `STORAGE=postgres`
  (startup-created schema `tasks(id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  title VARCHAR(200) NOT NULL, done BOOLEAN NOT NULL DEFAULT FALSE)`), both paths
  returning identical results. Postgres startup retry: 30 attempts × 1 s. Health in
  postgres mode pings the database (`SELECT 1`) before answering.
- Graceful shutdown on SIGTERM/SIGINT with an 8 s forced-exit fail-safe.
- Env vars unchanged: `STORAGE`, `PORT` (default 3000), `APP_VERSION` (default 1.0),
  standard `PG*` connection variables (`PGHOST`, `PGPORT`, `PGUSER`, `PGPASSWORD`, `PGDATABASE`).
- Deployment compatibility: single runnable Docker image; `.github/workflows/deploy.yml`
  (Docker Hub push → Render hook) keeps working.

**Dependency versions verified 2026-11-25** (crates.io / npm registries plus Context7
documentation: `/tokio-rs/axum`, `/tokio-rs/tokio`, `/transact-rs/sqlx`,
`/tower-rs/tower-http`, `/reactjs/react.dev`, `/vitejs/vite`, `/microsoft/typescript`):

| Package | Latest stable | Notes |
|---|---|---|
| `axum` | 0.8.9 | Routing, extractors, `IntoResponse` error type |
| `tokio` | 1.54.1 | Features: `macros`, `signal`, `time`, `net` |
| `sqlx` | 0.9.1 | Maintained at `transact-rs/sqlx`; runtime queries (no `query!` macros, so Docker builds need no live DB); features `runtime-tokio`, `postgres` |
| `tower-http` | 0.7.1 | `set-header` for security headers, `fs` for static assets |
| `react` | 19.3.1 | View Transitions / Fragment Refs newly stable — not used; baseline needs none |
| `vite` | 8.3.8 | `create-vite` react-ts template |
| `typescript` | 7.0.2 | Native compiler line; pin exact version at scaffold time |
| `async-trait` | 0.1.92 | Object-safe dyn dispatch for `TaskStore` (added in Phase 4) |
| `libc` (dev, unix-only) | 0.2.191 | `kill()` for SIGTERM/SIGINT shutdown tests (Phase 5) |
| `serial_test` (dev) | 4.0.1 | Serialize live-DB integration tests sharing one schema (Phase 5) |
| `tower` (runtime, `util`) | 0.5.3 | `service_fn` fallback for `ServeDir` static 404s (Phase 7) |

Out of scope: new features of any kind. Legacy behavior is frozen; nothing is added
that the baseline does not have. Per the language policy, Rust/React error messages,
logs and UI copy are English (message strings differ from the Spanish baseline; the
`{ error: string }` shape and status codes do not).

## 2. Delta Spec

### [ADDED]

- **`backend/` — Rust crate**: `src/main.rs` (env parsing, storage selection, graceful
  shutdown), `src/app.rs` (router assembly, middleware layers), `src/routes/` (health,
  tasks), `src/state.rs` (shared `AppState`), `src/storage/` (`TaskStore` trait +
  `MemoryStore` + `PostgresStore`), `src/error.rs` (single `AppError` type mapping to
  400/404/413/415/503 with `{ error }` body via `IntoResponse`),
  `src/validation.rs` (id and title rules). `cargo fmt` + `cargo clippy -D warnings`
  clean.
- **Backend test suite**: unit tests for validation/error mapping; HTTP integration
  tests using `tower::ServiceExt::oneshot` against the real router covering every
  endpoint, status semantic, header, and JSON shape above; postgres-path tests run
  only when a `DATABASE_URL`-style test database is present (same schema, isolated).
- **`frontend/` — React + Vite + TypeScript app**: `src/` component tree
  (`App`, task list, task item, create form, toggle/delete actions), `src/api.ts`
  fetch client. User data rendered only through JSX text nodes — no
  `dangerouslySetInnerHTML`. Title input `maxLength={200}` kept in sync with server
  validation. No external state library.
- **Updated multi-stage `Dockerfile`**: stage 1 `node:24-bookworm-slim` builds
  `frontend/dist`; stage 2 `rust` (recent stable) builds the backend with a cached
  dependency layer; stage 3 slim runtime containing the binary + static assets,
  `EXPOSE 3000`, `STORAGE` default `memory`. One image serves API and UI.
- **Static asset parity**: `GET /` (HTML), plus built assets served with the same
  security headers; the React bundle replaces `style.css`/`client.js` routes.

### [MODIFIED]

- **`.github/workflows/deploy.yml`**: add a test/lint gate job that runs
  `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (memory mode),
  `tsc --noEmit` + `vite build`, before any Docker build/push/Render hook; on `main`
  the publish flow (`:latest` + `:<sha>`) stays otherwise unchanged.
- **`README.md`** (single source of documentation): new architecture section, target
  stack commands (`cargo run`, `npm`/`vite` dev server, Docker build/run for both
  storage modes), env var table, API usage — all English.
- **`Dockerfile.multistage`** (teaching variant): updated in lockstep so both
  Dockerfiles keep building a runnable image; no behavior divergence.

### [REMOVED] — only after the parity test suite passes

- `server.js` (legacy Node HTTP server, including its Spanish strings and startup log).
- `package.json`, `package-lock.json`, and the Node `engines: >=24 <25` pin at repo
  root (the `pg@8.16.3` dependency disappears with them).
- `public/` (`index.html`, `style.css`, `client.js`) — replaced by `frontend/dist`
  served by the Rust backend.

## 3. Checklist

### Phase 0 — Spec & toolchain

- [x] Verify latest stable versions and idiomatic patterns for all seven dependencies via Context7 and crate/npm registries.
- [x] Create this living spec at `.specs/rust-react-migration.md`.
- [x] Verify a recent stable Rust toolchain is available on the host (`rustc --version`). Host has no Rust toolchain; per repo convention cargo runs in `rust:1-slim` (rustc 1.100.0) with cached volumes `cofla-cargo`, `cofla-target`, `cofla-rustup`.
- [x] Initialize `backend/` crate with `axum 0.8.9`, `tokio 1.54.1`, `sqlx 0.9.1`, `tower-http 0.7.1`, `serde`, `serde_json` pinned in `Cargo.toml`.

### Phase 1 — Backend skeleton + `/health` (TDD)

- [x] Write a failing integration test: `GET /health` returns 200 with `{ ok: true, storage: "memory", version: "<APP_VERSION or 1.0>" }` and `Cache-Control: no-store`.
- [x] Implement `main.rs` env parsing (`STORAGE`, `PORT`, `APP_VERSION`) + router skeleton + health handler until the test passes.
- [x] Write a failing test: `STORAGE=postgres` health returns 503 when the database is unreachable; make it pass with the store-selection stub.
- [x] Run `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` — all clean.

### Phase 2 — Tasks API, memory store (TDD)

- [x] Failing test: `GET /api/tasks` returns `[]` then created tasks ordered by id.
- [x] Failing test: `POST /api/tasks` with valid title returns 201 and `{ id, title, done: false }`; id increments from 1.
- [x] Failing test: `POST` with empty/whitespace/201-char title returns 400 `{ error }`.
- [x] Failing test: `PATCH /api/tasks/:id` with boolean `done` returns 200 updated task; unknown id returns 404; non-boolean `done` returns 400.
- [x] Failing test: `DELETE /api/tasks/:id` returns 200 `{ ok: true }` on success, 404 `{ error }` on unknown id.
- [x] Implement `MemoryStore` + routes until all Phase 2 tests pass; fmt/clippy/test clean.

### Phase 3 — Error & header semantics (TDD)

- [x] Failing tests: id `0` or above `2147483647` or non-safe-length numeric → 400 `Invalid ID`; non-numeric id path (baseline `\d+` regex miss, e.g. `abc`, `-5`) → 404. Implemented and passing in Phase 2 (`tasks_memory.rs`); confirm no regression.
- [x] Failing tests: wrong method on a known path and unknown route → 404 (never 405).
- [x] Failing test: request without `application/json` content type on POST/PATCH → 415.
- [x] Failing test: body over 8192 bytes → 413; malformed JSON → 400.
- [x] Failing test: any unexpected internal failure → 503 with `{ "error": "Service unavailable" }`.
- [x] Add tower-http set-header layer: CSP string and `nosniff` on every response, `no-store` on JSON; test header presence on success and error responses.

### Phase 4 — Postgres store (TDD)

- [x] Define `TaskStore` trait; make all memory tests generic over the trait. (Implemented as `Arc<dyn TaskStore>` via `async-trait 0.1.92` — plain `async fn` in trait is not object-safe; handlers now call the trait for every storage mode, so memory suites exercise the trait path.)
- [x] Implement `PostgresStore`: pool from standard `PG*` env vars (`postgres_url_from_env`), startup `CREATE TABLE IF NOT EXISTS` with the exact baseline schema (`SCHEMA_SQL`), 30 × 1 s retry (`connect_with_retry`), hard failure with English log message when the DB never answers; runtime `sqlx::query` API only, no compile-time macros.
- [x] Integration test against a throwaway Postgres: list/create/patch/delete results identical to memory-mode results (`tests/tasks_postgres.rs`, gated on `TEST_DATABASE_URL`, skips cleanly without it; verified green against `postgres:17-alpine`).
- [x] Health test with live Postgres: `SELECT 1` ping via `store.ping()` succeeds → 200 with `storage: "postgres"`; pool closed/dropped → tasks and health both return 503 `{ "error": "Service unavailable" }`.

### Phase 5 — Runtime behavior

- [x] Graceful shutdown: SIGTERM/SIGINT drain with 8 s forced-exit fail-safe; test shutdown sequence on the memory store (`tests/shutdown.rs` spawns the real binary with `PORT=0`, checks `/health` over raw TCP, signals via `libc::kill`, asserts exit code 0 within the fail-safe budget).
- [x] Startup log line in English including version, actually bound port, and storage mode; verified for both memory and postgres modes (postgres leg gated on `TEST_DATABASE_URL`). Postgres pool is closed on shutdown via `TaskStore::close`.

### Phase 6 — Frontend scaffold

- [x] Scaffold `frontend/` with Vite 8 + React 19 + TypeScript template; build passes. (Manual create-vite-equivalent scaffold; all tooling runs in `node:24-bookworm-slim` with cached `cofla-npm` volume because host Node is v26, outside the engines pin. Versions verified via npm + Context7: react/react-dom 19.3.1, vite 8.3.8, @vitejs/plugin-react 6.3.0, typescript 7.0.2, vitest 5.1.2, jsdom 31.0.1, @testing-library/react 16.3.3, @testing-library/dom 10.4.2, @testing-library/jest-dom 8.0.0.)
- [x] Implement `api.ts` fetch client matching the five endpoints; component tree renders tasks, add form with `maxLength={200}`, toggle and delete controls — no `dangerouslySetInnerHTML`. (`src/api/tasks.ts` typed client surfacing API `{ error }` messages; `App`, `NewTaskForm`, `TaskList`, `TaskItem`, `ErrorBanner`; English UI copy; dev proxy for `/api` and `/health` to `localhost:3000`.)
- [x] Frontend type-check (`tsc --noEmit`) and production build (`vite build`) pass. TDD: 11 Vitest component tests (`src/App.test.tsx`) written first against a stub (red 11/11), now green; CSS design tokens in `index.css` with no markup comments.

### Phase 7 — Static serving & full parity

- [x] Backend serves `frontend/dist` at `/` with security headers; integration test asserts HTML content type. (`tower-http` `ServeDir` via `AppState.static_dir`, `STATIC_DIR` env default `dist`; 404s from the file service go through a JSON `{"error":"Not found"}` fallback with `no-store`; wrong methods on static paths call the fallback instead of `405` via `call_fallback_on_method_not_allowed(true)` + `not_found_service`.)
- [x] End-to-end smoke against `STORAGE=memory` and `STORAGE=postgres`: both UI and API work — automated in `tests/static_assets.rs` (real binary, `STATIC_DIR` env, raw TCP) and by a binary-level live run: startup log `Cofla Tareas e2e-7 | port <ephemeral> | storage postgres`, `GET /` = 200 HTML with exact CSP/nosniff, health reports `storage`/`version`, create/list round-trip, unknown path JSON 404, SIGTERM exit 0.
- [x] Re-run the complete baseline contract test suite against the Rust server; 100% pass. (44/44 tests, verified both without DB [Postgres suites skip] and against live `postgres:17-alpine`.)

### Phase 8 — Docker & CI

- [x] Rewrite `Dockerfile` as the three-stage build (node:24 frontend `npm ci` + `npm run build` → rust:1-slim backend release build with dependency-layer-cached dummy `src/main.rs` → debian:bookworm-slim runtime with `ca-certificates`, non-root `appuser` uid 10001, `STATIC_DIR=/app/dist`, `PORT=3000`, `HEALTHCHECK` on `/health`, exec-form entrypoint for signal handling); `docker build` + run + smoke passes for both storage modes (134 MB image; `GET /` 200 HTML, `GET /health`, `POST` 201, list round-trip, unknown route 404, `docker stop` → exit 0 in 0.3 s with English shutdown logs; Postgres leg via shared network; HEALTHCHECK reached `healthy`).
- [x] Update `Dockerfile.multistage` teaching variant (same three stages, dependency layer rebuilt with sources for contrast); both images build and run (134 MB each).
- [x] Add the `test` job to `deploy.yml`: frontend `npm ci` + `tsc --noEmit` + `npm test` + `npm run build`, backend `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` with a `postgres:17-alpine` service container and `TEST_DATABASE_URL`; `build-and-deploy` gated via `needs: test`; publish flow (`:latest` + `:<sha>`, Render hook) otherwise unchanged; YAML validated with js-yaml.
- [ ] Verify on the runner that the workflow fails on an intentionally broken test, then passes on green (requires a push to GitHub; local semantics verified via the same command set).

### Phase 9 — Legacy removal

- [x] Confirm every Phase 7 parity test passes before any deletion. (Full backend suite re-run with live `postgres:17-alpine`: 44/44 green before touching the legacy files.)
- [x] Remove `server.js`, root `package.json`, `package-lock.json`, and `public/`. (`git rm`; root now contains only the target-stack layout; both Dockerfiles, `.dockerignore`, and `deploy.yml` no longer reference them — verified by re-running every suite post-deletion.)
- [x] Update `README.md` to target-stack-only documentation; final full-suite run green. (README rewritten as the single source of documentation: architecture tree, env vars, API/validation/error contract, security headers, schema, dev/test/Docker/CI-CD flows. `AGENTS.md` refreshed from migration rules to post-migration maintenance contract with exact commands and current gotchas. Final verification after deletion: frontend `tsc --noEmit` + 11/11 Vitest + build green; backend fmt/clippy/-D warnings + 44/44 tests with live Postgres green; zero-comment audit clean across `.rs`, `.ts`, `.tsx`, `.css`, `.html`, JSON, `Dockerfile`, `Dockerfile.multistage`, `deploy.yml`, `.dockerignore`.)
