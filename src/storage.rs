use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Result};
use deadpool::managed::{Hook, HookError, HookErrorCause, Pool};
use deadpool_sqlite::{Config, Manager, Runtime};
use deadpool_sync::SyncWrapper;
use rusqlite::{named_params, params, Connection};
use time::{Duration, OffsetDateTime};

use crate::models::{Span, TagValue, TraceId};

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
    async fn interact<F, T, E>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Into<anyhow::Error> + Send + Sync + 'static,
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
        let buf = zstd::encode_all(&*buf, 11)?;

        self.interact(move |conn| {
            conn.execute(
                include_str!("queries/save_span.sql"),
                params![
                    span.trace_id.to_bytes(),
                    &span.process.service,
                    &span.operation_name,
                    span.start,
                    span.duration.whole_microseconds() as u64,
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

    pub async fn list_spans(&self, params: ListSpansParams) -> Result<HashMap<TraceId, Vec<Span>>> {
        self.interact::<_, _, anyhow::Error>(move |conn| {
            conn.prepare(include_str!("queries/list_spans.sql"))?
                .query_map(
                    named_params! {
                        ":service": params.service,
                        ":op": params.operation,
                        ":t_min": params.start,
                        ":t_max": params.end,
                        ":d_min": params.duration_min.map(|d| d.whole_microseconds() as u64),
                        ":d_max": params.duration_max.map(|d| d.whole_microseconds() as u64),
                    },
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?
                .try_fold(HashMap::<TraceId, Vec<Span>>::new(), |mut map, entry| {
                    if map.len() >= params.limit {
                        return Ok(map);
                    }

                    let (trace_id, span) = trace_id_and_span(entry?)?;

                    if span_contains_tag(&span, &params.tags) {
                        map.entry(trace_id).or_default().push(span);
                    }

                    Ok(map)
                })
        })
        .await
    }

    pub async fn find_trace(&self, trace_id: TraceId) -> Result<Vec<Span>> {
        let spans = self
            .interact(move |conn| {
                conn.prepare(include_str!("queries/find_trace.sql"))?
                    .query_map([trace_id.to_bytes()], |row| row.get::<_, Vec<u8>>(0))?
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

    pub async fn find_traces(
        &self,
        trace_ids: impl Iterator<Item = TraceId>,
    ) -> Result<HashMap<TraceId, Vec<Span>>> {
        use std::rc::Rc;

        use rusqlite::types::Value;

        let trace_ids = trace_ids.map(Into::into).collect::<Vec<Value>>();

        self.interact::<_, _, anyhow::Error>(move |conn| {
            conn.prepare(include_str!("queries/find_traces.sql"))?
                .query_map([Rc::new(trace_ids)], |row| Ok((row.get(0)?, row.get(1)?)))?
                .try_fold(HashMap::<TraceId, Vec<Span>>::new(), |mut map, entry| {
                    let (trace_id, span) = trace_id_and_span(entry?)?;
                    map.entry(trace_id).or_default().push(span);
                    Ok(map)
                })
        })
        .await
    }
}

fn trace_id_and_span((trace_id, span): ([u8; 16], Vec<u8>)) -> Result<(TraceId, Span)> {
    let trace_id = TraceId::try_from(trace_id).unwrap();
    let span = zstd::decode_all(&*span)?;
    let span = postcard::from_bytes(&span)?;

    Ok((trace_id, span))
}

pub struct ListSpansParams {
    pub service: String,
    pub operation: Option<String>,
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
    pub duration_min: Option<Duration>,
    pub duration_max: Option<Duration>,
    pub limit: usize,
    pub tags: HashMap<String, String>,
}

fn span_contains_tag(span: &Span, filter: &HashMap<String, String>) -> bool {
    if filter.is_empty() {
        return true;
    }

    span.tags
        .iter()
        .chain(span.process.tags.iter())
        .any(|tag| match filter.get(&tag.key) {
            Some(value) => match &tag.value {
                TagValue::String(s) => value == s,
                TagValue::Bool(b) => value == if *b { "true" } else { "false" },
                TagValue::I64(i) => value == &i.to_string(),
                TagValue::F64(f) => value == &f.to_string(),
                TagValue::Binary(b) => value == &hex::encode(b),
            },
            None => false,
        })
}
