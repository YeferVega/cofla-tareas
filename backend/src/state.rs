use std::path::PathBuf;
use std::sync::Arc;

use crate::storage::{MemoryStore, PostgresStore, StoreError, TaskStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageMode {
    Memory,
    Postgres,
}

impl StorageMode {
    pub fn name(&self) -> &'static str {
        match self {
            StorageMode::Memory => "memory",
            StorageMode::Postgres => "postgres",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub mode: StorageMode,
    pub version: String,
    pub store: Arc<dyn TaskStore>,
    pub static_dir: PathBuf,
}

impl AppState {
    pub fn with_store(
        mode: StorageMode,
        version: impl Into<String>,
        store: Arc<dyn TaskStore>,
    ) -> Self {
        Self {
            mode,
            version: version.into(),
            store,
            static_dir: PathBuf::from("dist"),
        }
    }

    pub fn with_static_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.static_dir = dir.into();
        self
    }

    pub fn memory(version: impl Into<String>) -> Self {
        Self::with_store(StorageMode::Memory, version, Arc::new(MemoryStore::new()))
    }

    pub fn postgres(version: impl Into<String>, url: &str) -> Result<Self, StoreError> {
        Ok(Self::with_store(
            StorageMode::Postgres,
            version,
            Arc::new(PostgresStore::connect_lazy(url)?),
        ))
    }

    pub fn mode_name(&self) -> &'static str {
        self.mode.name()
    }
}
