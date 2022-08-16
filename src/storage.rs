use std::sync::Arc;

use anyhow::{anyhow, Result};
use archer_thrift::{jaeger::Batch, thrift::protocol::TCompactOutputProtocol};
use deadpool::managed::{Hook, HookError, HookErrorCause, Pool};
use deadpool_sqlite::{Config, Manager, Runtime};
use deadpool_sync::SyncWrapper;
use rusqlite::Connection;

#[derive(Clone)]
pub struct Database(Arc<Pool<Manager>>);

pub async fn init() -> Result<Database> {
    let pool = Config::new("db.sqlite3")
        .builder(Runtime::Tokio1)?
        .post_create(async_fn(|conn: &mut Connection| {
            conn.execute_batch(
                "
                PRAGMA journal_mode = wal;
                PRAGMA synchronous = normal;
                PRAGMA foreign_keys = on;
                ",
            )
        }))
        .pre_recycle(async_fn(|conn: &mut Connection| {
            conn.execute_batch(
                "
                PRAGMA analysis_limit = 400;
                PRAGMA optimize;
                ",
            )
        }))
        .build()?;

    pool.get()
        .await?
        .interact(|conn| {
            conn.execute(include_str!("queries/00_create.sql"), [])?;
            anyhow::Ok(())
        })
        .await
        .map_err(|e| anyhow!("{e}"))??;

    Ok(Database(Arc::new(pool)))
}

fn async_fn(f: fn(&mut Connection) -> rusqlite::Result<()>) -> Hook<Manager> {
    Hook::async_fn(move |conn: &mut SyncWrapper<Connection>, _| {
        Box::pin(async move {
            conn.interact(f)
                .await
                .map_err(|e| HookError::Abort(HookErrorCause::Message(e.to_string())))?
                .map_err(|e| HookError::Abort(HookErrorCause::Backend(e)))
        })
    })
}

impl Database {
    pub async fn save_span(&self, batch: Batch) -> Result<()> {
        let mut buf = Vec::new();
        let mut prot = TCompactOutputProtocol::new(&mut buf);

        batch.write_to_out_protocol(&mut prot)?;

        self.0
            .get()
            .await?
            .interact(|conn| conn.execute(include_str!("queries/save_span.sql"), [buf]))
            .await
            .map_err(|e| anyhow!("{e}"))??;

        Ok(())
    }
}
