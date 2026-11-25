use std::env;
use std::sync::Arc;
use std::time::Duration;

use cofla_tareas_backend::app::build_router;
use cofla_tareas_backend::state::{AppState, StorageMode};
use cofla_tareas_backend::storage::{postgres_url_from_env, MemoryStore, PostgresStore, TaskStore};

const STARTUP_RETRY_ATTEMPTS: u32 = 30;
const STARTUP_RETRY_DELAY: Duration = Duration::from_secs(1);
const SHUTDOWN_FAIL_SAFE: Duration = Duration::from_secs(8);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let version = env::var("APP_VERSION").unwrap_or_else(|_| "1.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let storage = env::var("STORAGE").unwrap_or_else(|_| "memory".to_string());
    let (mode, store): (StorageMode, Arc<dyn TaskStore>) = match storage.as_str() {
        "memory" => (StorageMode::Memory, Arc::new(MemoryStore::new())),
        "postgres" => {
            let url = postgres_url_from_env();
            let store = PostgresStore::connect_with_retry(
                &url,
                STARTUP_RETRY_ATTEMPTS,
                STARTUP_RETRY_DELAY,
            )
            .await
            .inspect_err(|_| eprintln!("Database did not respond; check PGHOST and credentials"))?;
            (StorageMode::Postgres, Arc::new(store))
        }
        other => {
            eprintln!("STORAGE must be memory or postgres, got {other}");
            std::process::exit(1);
        }
    };
    let state = AppState::with_store(mode, version, Arc::clone(&store))
        .with_static_dir(env::var("STATIC_DIR").unwrap_or_else(|_| "dist".to_string()));
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    let bound_port = listener.local_addr()?.port();
    println!(
        "Cofla Tareas {} | port {} | storage {}",
        state.version,
        bound_port,
        state.mode_name()
    );
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_guard())
        .await?;
    store.close().await;
    println!("Shutdown complete | storage {}", mode.name());
    Ok(())
}

async fn shutdown_guard() {
    wait_for_signal().await;
    println!("Shutdown signal received, draining connections");
    tokio::spawn(async move {
        tokio::time::sleep(SHUTDOWN_FAIL_SAFE).await;
        eprintln!("Graceful shutdown did not finish in time; forcing exit");
        std::process::exit(1);
    });
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if result.is_err() {
                    eprintln!("failed to listen for ctrl-c");
                }
            }
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        if tokio::signal::ctrl_c().await.is_err() {
            eprintln!("failed to listen for ctrl-c");
        }
    }
}
