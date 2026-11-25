FROM node:24-bookworm-slim AS frontend-build
WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1-slim AS backend-build
WORKDIR /build
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs
COPY backend/ ./
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --shell /usr/sbin/nologin appuser
WORKDIR /app
COPY --from=backend-build /build/target/release/cofla-tareas-backend /app/cofla-tareas
COPY --from=frontend-build /build/dist /app/dist
ENV STORAGE=memory PORT=3000 APP_VERSION=1.0 STATIC_DIR=/app/dist
EXPOSE 3000
USER appuser
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/3000 && printf 'GET /health HTTP/1.1\\r\\nHost: healthcheck\\r\\nConnection: close\\r\\n\\r\\n' >&3 && head -n 1 <&3 | grep -q 200"]
ENTRYPOINT ["/app/cofla-tareas"]
