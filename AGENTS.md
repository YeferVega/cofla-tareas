# AGENTS.md — cofla-tareas

## Project status
- Migration complete: **Rust** backend (`backend/`, axum 0.8 + tokio 1.54 +
  sqlx 0.9 + tower-http 0.7) and **React** frontend (`frontend/`, React 19.3 +
  TypeScript 7 + Vite 8). The legacy Node.js server and `public/` are deleted.
- `README.md` is the single source of documentation and the behavioral
  reference. `.specs/rust-react-migration.md` records the completed migration.

## Behavioral contract — do not break
- Endpoints: `GET /health`, `GET /` (static), `GET|POST /api/tasks`,
  `PATCH|DELETE /api/tasks/:id`; task shape `{ id, title, done }`; health shape
  `{ ok, storage, version }`; delete success `{ ok: true }`; errors
  `{ error: string }`.
- Status semantics: 400 invalid input/ID, 404 unknown route **and wrong
  method** (no 405 anywhere), 413 body > 8192 bytes, 415 non-JSON content
  type, 503 unknown failures (never 500).
- ID: all-digit path segment `1..=2147483647` → else 400; non-numeric → 404.
  Title: 1–200 chars trimmed; DB column, server validation, and UI
  `maxLength={200}` stay in sync. PATCH updates `done` only.
- Dual storage `STORAGE=memory|postgres`, identical results; startup-created
  schema (no migrations tool); Postgres startup retry 30 × 1 s; health pings
  the DB (`SELECT 1`) in postgres mode.
- Security headers everywhere: CSP
  `default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'`,
  `X-Content-Type-Options: nosniff`, `Cache-Control: no-store` on JSON.
- Graceful SIGTERM/SIGINT shutdown, 8 s fail-safe, pool close, exit 0.
- Env vars: `STORAGE`, `PORT` (3000), `APP_VERSION` (1.0), `STATIC_DIR`
  (`dist`; `/app/dist` in Docker), standard `PG*` variables.
- Deployment: both `Dockerfile` and `Dockerfile.multistage` must keep building
  a runnable image; the Docker Hub push + Render hook flow stays working.

## Language policy — strict English
- All code, tests, UI copy, API error messages, logs, specs, agent
  communication, Dockerfiles, and workflows are English only.

## Zero-comment policy
- No comments anywhere in Rust, TypeScript/JavaScript, CSS, HTML, test files,
  Dockerfiles, workflows, or `.dockerignore`: no `//`, `/* */`, `///`,
  `{/* */}`, `#`, or markup comments.
- `README.md` is the **single source of documentation**: architecture, setup,
  environment variables, API usage. No other docs files except Tier B specs.
- If something feels like it needs a comment, rename or restructure the code
  instead.

## Methodology — TDD + adaptive delta-specs
- **Tier A (small/local change):** no spec files. Inspect → write a failing
  test → implement minimal code → run the full test suite and linter/formatter.
- **Tier B (cross-cutting feature):** one living file `.specs/<feature>.md`
  with exactly three sections: Goal & Scope; Delta Spec tagged `[ADDED]` /
  `[MODIFIED]` / `[REMOVED]`; atomic `[ ]`/`[x]` checklist, one verifiable
  action each. Update the same file as work proceeds.
- Before adding or using any crate/npm package, verify its recent stable
  version and current API in official docs (docs.rs / package docs). Never
  code from memory of older APIs.
- Red must fail for behavioral reasons. Legacy-parity rules from the completed
  migration still apply: never silently extend the frozen contract.

## Commands
| Purpose | Command |
|---|---|
| Backend run | `cargo run` (from `backend/`) |
| Backend check | `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test` |
| Backend live DB tests | `TEST_DATABASE_URL=postgres://... cargo test` |
| Frontend dev | `npm ci` · `npm run dev` (from `frontend/`, proxies to :3000) |
| Frontend check | `npx tsc --noEmit` · `npm test` · `npm run build` |
| Build image | `docker build -t cofla-tareas .` |
| Run (memory) | `docker run -p 3000:3000 cofla-tareas` |
| Smoke check | `curl http://localhost:3000/health` |

## Gotchas
- The host has **no Rust toolchain** and Node outside the `>=24 <25` pin: run
  cargo in `rust:1-slim` and npm in `node:24-bookworm-slim` containers with
  bind mounts and named cache volumes.
- `rust:1-slim` sets `CARGO_HOME=/usr/local/cargo` — mount cache volumes
  there, not `/root/.cargo`.
- Postgres-gated suites share one schema: keep `#[serial]` on tests that
  truncate; live CI runs them via the `postgres:17-alpine` service container.
- `deploy.yml` gates publishing on the `test` job; pushing `main` with red
  tests will not publish.
- Frontend deps (`@types/react`, `@testing-library/dom`) are peer/type
  packages React 19 does not bundle; keep them in `frontend/package.json`.
- `npm ci` only (frontend lockfile v3 committed; `engines: >=24 <25` pinned in
  `frontend/package.json`).
