# Cofla Tareas

A small shared task list. Rust backend, React frontend, single Docker image.

## Architecture

```
.
├── backend/                 Rust (axum 0.8, tokio 1.54, sqlx 0.9, tower-http 0.7)
│   ├── src/main.rs          Env parsing, storage selection, graceful shutdown
│   ├── src/app.rs           Router, security headers, static file service
│   ├── src/routes/          health + tasks handlers, shared not-found
│   ├── src/storage.rs       TaskStore trait: MemoryStore | PostgresStore
│   ├── src/state.rs         AppState (mode, version, store, static dir)
│   ├── src/error.rs         AppError → 400/404/413/415/503 JSON mapping
│   ├── src/json.rs          JSON response helper (content-type + no-store)
│   └── tests/               HTTP integration suites (memory, postgres, parity, shutdown)
├── frontend/                React 19.3 + TypeScript 7 + Vite 8
│   ├── src/App.tsx          Task list orchestration
│   ├── src/api/tasks.ts     Typed fetch client
│   ├── src/components/      NewTaskForm, TaskList, TaskItem, ErrorBanner
│   ├── src/index.css        Design tokens and layout
│   └── src/App.test.tsx     Vitest + Testing Library suite
├── Dockerfile               Three-stage production build
├── Dockerfile.multistage    Teaching variant
└── .github/workflows/       CI: test gate → Docker Hub push → Render redeploy
```

The backend serves the API and the built frontend from the same process. The
Vite dev server proxies `/api` and `/health` to `http://localhost:3000`.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `STORAGE` | `memory` | `memory` (volatile) or `postgres`. Any other value exits with an error. |
| `PORT` | `3000` | Listen port. `0` binds an ephemeral port (logged on startup). |
| `APP_VERSION` | `1.0` | Reported by `/health` and the startup log. |
| `STATIC_DIR` | `dist` | Directory holding the frontend build. `/app/dist` in Docker. |
| `PGHOST` | `localhost` | Postgres host. |
| `PGPORT` | `5432` | Postgres port. |
| `PGUSER` | `postgres` | Postgres user. |
| `PGPASSWORD` | — | Omitted from the URL when unset. |
| `PGDATABASE` | `$PGUSER` | Database name. |

In `postgres` mode the server retries schema creation 30 times with a 1-second
delay at startup, then exits with an English error if the database never
answered. On SIGTERM/SIGINT it drains connections, closes the pool, and exits
0, forcing exit if draining exceeds 8 seconds.

## API

All requests and responses are JSON. Every response carries the security
headers below; unknown internal failures map to 503 (never 500).

| Method | Path | Behavior |
|---|---|---|
| `GET` | `/health` | `200 {"ok":true,"storage":"memory"\|"postgres","version":"<APP_VERSION>"}`. In postgres mode pings the DB; unreachable DB → `503`. |
| `GET` | `/` | `200` `index.html` from `STATIC_DIR` (other unknown paths → `404` JSON, never SPA fallback). |
| `GET` | `/api/tasks` | `200` `[{"id","title","done"}]` ordered by id. |
| `POST` | `/api/tasks` | Body `{"title":"..."}` → `201 {"id","title","done":false}`. |
| `PATCH` | `/api/tasks/:id` | Body `{"done":true\|false}` → `200` updated task (title is not changed by PATCH). |
| `DELETE` | `/api/tasks/:id` | `200 {"ok":true}`, or `404` if unknown. |

Validation and error contract:

- `title`: 1–200 characters after trimming → otherwise `400`.
- `id`: all-digit path segment in `1..=2147483647`; out-of-range → `400
  {"error":"Invalid ID"}`; non-numeric (e.g. `abc`, `-5`) → `404`.
- `done` on PATCH must be a boolean → otherwise `400`.
- Body larger than 8192 bytes → `413`. Content type not starting with
  `application/json` → `415`. Malformed JSON → `400`.
- Unknown route or wrong HTTP method → `404 {"error":"Not found"}` (no 405).
- Storage/database failure → `503 {"error":"Service unavailable"}`.

Security headers on every response:

```
Content-Security-Policy: default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'
X-Content-Type-Options: nosniff
Cache-Control: no-store        (on all JSON responses)
```

Storage schema, created automatically at startup:

```sql
CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  title VARCHAR(200) NOT NULL,
  done BOOLEAN NOT NULL DEFAULT FALSE
)
```

`memory` and `postgres` modes return identical results for identical calls.

## Local development

Requires Rust (stable) and Node 24 (`>=24 <25`). If the host toolchain does not
match, run them in containers, e.g. `rust:1-slim` and `node:24-bookworm-slim`.

Backend (from `backend/`):

```sh
cargo run                  # STORAGE=memory by default
STORAGE=postgres cargo run # needs a reachable Postgres via PG* vars
```

Frontend (from `frontend/`):

```sh
npm ci
npm run dev                # Vite dev server on :5173, proxying /api and /health
```

## Tests

Backend (from `backend/`):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                 # Postgres suites skip unless TEST_DATABASE_URL is set
```

To run the Postgres suites against a live database:

```sh
docker run -d --name pg-test -e POSTGRES_USER=cofla -e POSTGRES_PASSWORD=cofla \
  -e POSTGRES_DB=cofla -p 5543:5432 postgres:17-alpine
TEST_DATABASE_URL=postgres://cofla:cofla@localhost:5543/cofla cargo test
```

Frontend (from `frontend/`):

```sh
npm test                   # Vitest + Testing Library
npx tsc --noEmit
npm run build              # emits frontend/dist
```

## Docker

The production `Dockerfile` has three stages: `node:24-bookworm-slim` builds
`frontend/dist`, `rust:1-slim` builds the release binary (dependency layer
cached via a stub `src/main.rs`), and `debian:bookworm-slim` runs both as a
non-root user with a `/health` healthcheck. `Dockerfile.multistage` is the
teaching variant with the same runtime and no dependency-layer stub.

```sh
docker build -t cofla-tareas .
docker run -p 3000:3000 cofla-tareas                                  # memory
docker network create tasks-net
docker run -d --name pg --network tasks-net \
  -e POSTGRES_USER=cofla -e POSTGRES_PASSWORD=cofla -e POSTGRES_DB=cofla \
  postgres:17-alpine
docker run -p 3000:3000 --network tasks-net -e STORAGE=postgres \
  -e PGHOST=pg -e PGUSER=cofla -e PGPASSWORD=cofla -e PGDATABASE=cofla \
  cofla-tareas                                                        # postgres
```

## CI/CD

`.github/workflows/deploy.yml` runs on every push to `main`:

1. `test` job — frontend install/typecheck/test/build and backend
   fmt/clippy/test, with a `postgres:17-alpine` service so the live-database
   suites execute.
2. `build-and-deploy` job (requires `test`) — builds the `Dockerfile` image and
   pushes `:latest` and `:<git-sha>` to Docker Hub.
3. Render redeploy — `curl`s the `RENDER_DEPLOY_HOOK` secret.

Required secrets: `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN`,
`RENDER_DEPLOY_HOOK`.

## Architecture & Agentic Engineering Workflow

### System Architecture

Two independent modules meet at one deployment artifact:

- **`backend/`** — Rust + Axum. Every HTTP route calls through a `TaskStore`
  trait, so `memory` and `postgres` (SQLx runtime queries, no compile-time
  macros) are interchangeable implementations behind one contract; the two
  modes return identical results for identical calls. Error handling is
  centralized in a single `AppError` type that maps to the fixed status
  semantics (400 / 404 / 413 / 415 / 503, never 405 or 500).
- **`frontend/`** — React 19 + TypeScript + Vite. `vite build` emits static
  assets that the Rust binary itself serves from `STATIC_DIR`, so the API and
  the UI ship in one process on one port.

Every response carries the security headers (`Content-Security-Policy`,
`X-Content-Type-Options: nosniff`, and `Cache-Control: no-store` on JSON).
The production image is a three-stage Docker build (`node` → `rust` →
`debian-slim`) whose final stage runs as a non-root user, keeps only
runtime dependencies, exposes the service on `PORT`, and health-checks
`/health`.

### Agentic Engineering Methodology (Adaptive Delta-Spec + TDD)

The repository is maintained agentically with proportionate ceremony:

- **Lightweight governance, fresh APIs.** `AGENTS.md` holds the behavioral
  contract, language and zero-comment policies, and gotchas — the always-on
  context is small by design. On-demand library truth comes from the
  **Context7 MCP**: before using any crate or package, its current stable
  version and idioms are verified against live docs instead of model memory,
  which is what kept axum 0.8, sqlx 0.9, React 19.3 and Vite 8 correct
  throughout this migration.
- **Tier A / Tier B.** Small fixes are handled inline: inspect, add a failing
  test, implement, re-run the full suite and linters — zero extra files.
  Cross-cutting work gets exactly one living delta-spec under `.specs/`
  (`Goal & Scope`, a `Delta Spec` with `[ADDED]` / `[MODIFIED]` / `[REMOVED]`
  items, and an atomic verifiable checklist) that is updated as progress
  lands rather than proliferating documents. The Rust + React migration was
  delivered as `.specs/rust-react-migration.md` through ten such phases.
- **Red → Green TDD, machine-gated.** No feature ships without a test that
  failed for behavioral reasons first. HTTP integration suites (memory and
  live Postgres), Vitest + Testing Library component suites, and container
  smoke checks gate every phase, alongside `cargo fmt --check`,
  `cargo clippy -- -D warnings` and `tsc --noEmit` under strict TypeScript.
  The codebase stays comment-free and self-documenting: names and structure
  carry the explanation, and `README.md` remains the single source of
  documentation.
