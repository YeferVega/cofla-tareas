use std::env;
use std::fmt;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::Row;

use crate::models::Task;

pub const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS tasks (id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, title VARCHAR(200) NOT NULL, done BOOLEAN NOT NULL DEFAULT FALSE)";

#[derive(Debug)]
pub enum StoreError {
    Unavailable,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Unavailable => write!(f, "task storage is unavailable"),
        }
    }
}

impl std::error::Error for StoreError {}

#[async_trait]
pub trait TaskStore: Send + Sync + fmt::Debug {
    async fn list(&self) -> Result<Vec<Task>, StoreError>;
    async fn create(&self, title: String) -> Result<Task, StoreError>;
    async fn set_done(&self, id: i32, done: bool) -> Result<Option<Task>, StoreError>;
    async fn remove(&self, id: i32) -> Result<bool, StoreError>;
    async fn ping(&self) -> Result<(), StoreError>;
    async fn close(&self) {}
}

#[derive(Debug)]
pub struct MemoryStore {
    next_id: AtomicI32,
    tasks: Mutex<Vec<Task>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            next_id: AtomicI32::new(1),
            tasks: Mutex::new(Vec::new()),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskStore for MemoryStore {
    async fn list(&self) -> Result<Vec<Task>, StoreError> {
        Ok(self.tasks.lock().unwrap().clone())
    }

    async fn create(&self, title: String) -> Result<Task, StoreError> {
        let task = Task {
            id: self.next_id.fetch_add(1, Ordering::SeqCst),
            title,
            done: false,
        };
        self.tasks.lock().unwrap().push(task.clone());
        Ok(task)
    }

    async fn set_done(&self, id: i32, done: bool) -> Result<Option<Task>, StoreError> {
        let mut tasks = self.tasks.lock().unwrap();
        let Some(task) = tasks.iter_mut().find(|task| task.id == id) else {
            return Ok(None);
        };
        task.done = done;
        Ok(Some(task.clone()))
    }

    async fn remove(&self, id: i32) -> Result<bool, StoreError> {
        let mut tasks = self.tasks.lock().unwrap();
        let before = tasks.len();
        tasks.retain(|task| task.id != id);
        Ok(tasks.len() != before)
    }

    async fn ping(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub fn connect_lazy(url: &str) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect_lazy(url)
            .map_err(|_| StoreError::Unavailable)?;
        Ok(Self { pool })
    }

    pub async fn connect_with_retry(
        url: &str,
        attempts: u32,
        delay: Duration,
    ) -> Result<Self, StoreError> {
        let store = Self::connect_lazy(url)?;
        for attempt in 1..=attempts {
            match sqlx::query(SCHEMA_SQL).execute(&store.pool).await {
                Ok(_) => return Ok(store),
                Err(err) => {
                    eprintln!("Waiting for database (attempt {attempt}/{attempts}): {err}");
                    if attempt == attempts {
                        store.pool.close().await;
                        return Err(StoreError::Unavailable);
                    }
                    tokio::time::sleep(delay).await;
                }
            }
        }
        Err(StoreError::Unavailable)
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl TaskStore for PostgresStore {
    async fn list(&self) -> Result<Vec<Task>, StoreError> {
        let rows = sqlx::query("SELECT id, title, done FROM tasks ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(|_| StoreError::Unavailable)?;
        Ok(rows.iter().map(task_from_row).collect())
    }

    async fn create(&self, title: String) -> Result<Task, StoreError> {
        let row = sqlx::query("INSERT INTO tasks (title) VALUES ($1) RETURNING id, title, done")
            .bind(title)
            .fetch_one(&self.pool)
            .await
            .map_err(|_| StoreError::Unavailable)?;
        Ok(task_from_row(&row))
    }

    async fn set_done(&self, id: i32, done: bool) -> Result<Option<Task>, StoreError> {
        let row = sqlx::query("UPDATE tasks SET done = $1 WHERE id = $2 RETURNING id, title, done")
            .bind(done)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| StoreError::Unavailable)?;
        Ok(row.map(|row| task_from_row(&row)))
    }

    async fn remove(&self, id: i32) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM tasks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|_| StoreError::Unavailable)?;
        Ok(result.rows_affected() > 0)
    }

    async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| StoreError::Unavailable)?;
        Ok(())
    }

    async fn close(&self) {
        self.pool.close().await;
    }
}

fn task_from_row(row: &PgRow) -> Task {
    Task {
        id: row.get("id"),
        title: row.get("title"),
        done: row.get("done"),
    }
}

pub fn postgres_url_from_env() -> String {
    let host = env_or("PGHOST", "localhost");
    let port = env_or("PGPORT", "5432");
    let user = env_or("PGUSER", "postgres");
    let database = env_or("PGDATABASE", &user);
    match env::var("PGPASSWORD") {
        Ok(password) if !password.is_empty() => {
            format!("postgres://{user}:{password}@{host}:{port}/{database}")
        }
        _ => format!("postgres://{user}@{host}:{port}/{database}"),
    }
}

fn env_or(key: &str, fallback: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
