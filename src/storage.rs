use std::sync::Arc;

use anyhow::{anyhow, Result};
use deadpool::managed::{Hook, HookError, HookErrorCause, Pool};
use deadpool_sqlite::{Config, Manager, Runtime};
use deadpool_sync::SyncWrapper;
use rusqlite::{params, Connection};

use crate::models::{Span, TraceId};

#[derive(Clone)]
pub struct Database(Arc<Pool<Manager>>);

pub async fn init() -> Result<Database> {
    let pool = Config::new("db.sqlite3")
        .builder(Runtime::Tokio1)?
        .post_create(async_fn(|conn: &mut Connection| {
            rusqlite::vtab::array::load_module(conn)?;
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
            conn.execute_batch(include_str!("queries/00_create.sql"))?;
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
    async fn interact<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T, rusqlite::Error> + Send + 'static,
        T: Send + 'static,
    {
        self.0
            .get()
            .await?
            .interact(f)
            .await
            .map_err(|e| anyhow!("{e}"))?
            .map_err(Into::into)
    }

    pub async fn save_span(&self, span: Span) -> Result<()> {
        let buf = postcard::to_stdvec(&span)?;
        let buf = zstd::bulk::compress(&buf, 11)?;

        self.interact(move |conn| {
            conn.execute(
                include_str!("queries/save_span.sql"),
                params![
                    span.trace_id.to_bytes(),
                    &span.process.service,
                    &span.operation_name,
                    buf,
                ],
            )
        })
        .await?;

        Ok(())
    }

    pub async fn list_services(&self) -> Result<Vec<String>> {
        self.interact(|conn| {
            conn.prepare(include_str!("queries/list_services.sql"))?
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .await
    }

    pub async fn list_operations(&self, service: String) -> Result<Vec<String>> {
        self.interact(|conn| {
            conn.prepare(include_str!("queries/list_operations.sql"))?
                .query_map([service], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .await
    }

    pub async fn list_spans(
        &self,
        service: String,
        _operation: Option<String>,
    ) -> Result<Vec<Span>> {
        let spans = self
            .interact(|conn| {
                conn.prepare(include_str!("queries/list_spans.sql"))?
                    .query_map([service], |row| row.get::<_, Vec<u8>>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await?;

        spans
            .into_iter()
            .map(|span| {
                let span = zstd::decode_all(&*span)?;
                let span = postcard::from_bytes(&span)?;
                anyhow::Ok(span)
            })
            .collect::<Result<Vec<_>>>()
    }

    pub async fn find_trace(&self, trace_id: u128) -> Result<Vec<Span>> {
        let spans = self
            .interact(move |conn| {
                conn.prepare(include_str!("queries/find_trace.sql"))?
                    .query_map([trace_id.to_be_bytes()], |row| row.get::<_, Vec<u8>>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await?;

        spans
            .into_iter()
            .map(|span| {
                let span = zstd::decode_all(&*span)?;
                let span = postcard::from_bytes(&span)?;
                anyhow::Ok(span)
            })
            .collect::<Result<Vec<_>>>()
    }

    pub async fn find_traces(&self, trace_ids: impl Iterator<Item = TraceId>) -> Result<Vec<Span>> {
        use std::rc::Rc;

        use rusqlite::types::Value;

        let trace_ids = trace_ids.map(Into::into).collect::<Vec<Value>>();

        let spans = self
            .interact(move |conn| {
                conn.prepare(include_str!("queries/find_traces.sql"))?
                    .query_map([Rc::new(trace_ids)], |row| row.get::<_, Vec<u8>>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await?;

        spans
            .into_iter()
            .map(|span| {
                let span = zstd::decode_all(&*span)?;
                let span = postcard::from_bytes(&span)?;
                anyhow::Ok(span)
            })
            .collect::<Result<Vec<_>>>()
    }
}
