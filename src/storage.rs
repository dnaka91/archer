use std::{collections::HashMap, rc::Rc, sync::Arc};

use anyhow::{anyhow, Context, Result};
use rusqlite::{named_params, params, types::Value, Connection};
use time::{Duration, OffsetDateTime};
use tokio::sync::Mutex;
use tracing::instrument;
use unidirs::{Directories, UnifiedDirs, Utf8PathBuf};

use crate::models::{Span, TagValue, TraceId};

#[derive(Clone)]
pub struct Database(Arc<Mutex<Connection>>);

pub async fn init() -> Result<Database> {
    let conn = tokio::task::spawn_blocking(|| {
        let path = get_db_path()?;
        let mut conn = Connection::open(path)?;

        conn.trace(Some(|sql| tracing::trace!("{sql}")));
        rusqlite::vtab::array::load_module(&conn)?;
        conn.execute_batch(include_str!("queries/00_pragmas.sql"))?;
        conn.execute_batch(include_str!("queries/01_create.sql"))?;

        anyhow::Ok(conn)
    })
    .await??;

    Ok(Database(Arc::new(Mutex::new(conn))))
}

fn get_db_path() -> Result<Utf8PathBuf> {
    let dirs = UnifiedDirs::simple("rocks", "dnaka91", env!("CARGO_PKG_NAME"))
        .default()
        .context("failed finding project directories")?;
    let data_dir = dirs.data_dir();

    std::fs::create_dir_all(&data_dir)?;

    Ok(data_dir.join("db.sqlite3"))
}

impl Database {
    async fn interact<F, T, E>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: Into<anyhow::Error> + Send + Sync + 'static,
    {
        let mut conn = Arc::clone(&self.0).lock_owned().await;

        tokio::task::spawn_blocking(move || f(&mut conn))
            .await
            .map_err(|e| anyhow!("{e}"))?
            .map_err(Into::into)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub async fn save_spans(&self, spans: Vec<Span>) -> Result<()> {
        let trace_info = TraceInfo::from_spans(&spans);

        self.interact::<_, _, anyhow::Error>(move |conn| {
            let conn = conn.transaction()?;

            {
                let mut stmt = conn.prepare_cached(include_str!("queries/save_service.sql"))?;
                for span in &spans {
                    stmt.execute([&span.process.service])?;
                }

                let mut stmt = conn.prepare_cached(include_str!("queries/save_operation.sql"))?;
                for span in &spans {
                    stmt.execute([&span.process.service, &span.operation_name])?;
                }

                let mut stmt = conn.prepare_cached(include_str!("queries/save_trace.sql"))?;
                for (trace_id, info) in trace_info {
                    stmt.execute(params![
                        trace_id.to_bytes(),
                        info.service,
                        info.timestamp,
                        info.min_duration.whole_microseconds() as u64,
                        info.max_duration.whole_microseconds() as u64
                    ])?;
                }

                let mut stmt = conn.prepare_cached(include_str!("queries/save_span.sql"))?;
                for span in spans {
                    let params = params![
                        span.trace_id.to_bytes(),
                        span.span_id.to_bytes(),
                        span.operation_name,
                        encode_span(&span)?,
                    ];
                    stmt.execute(params)?;
                }
            }

            conn.commit().map_err(Into::into)
        })
        .await
    }

    #[instrument(skip_all)]
    pub async fn list_services(&self) -> Result<Vec<String>> {
        self.interact(|conn| {
            conn.prepare(include_str!("queries/list_services.sql"))?
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .await
    }

    #[instrument(skip_all)]
    pub async fn list_operations(&self, service: String) -> Result<Vec<String>> {
        self.interact(|conn| {
            conn.prepare(include_str!("queries/list_operations.sql"))?
                .query_map([service], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .await
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[instrument(skip_all)]
    pub async fn list_spans(&self, params: ListSpansParams) -> Result<HashMap<TraceId, Vec<Span>>> {
        self.interact::<_, _, anyhow::Error>(move |conn| {
            let trace_ids = conn
                .prepare(include_str!("queries/list_traces.sql"))?
                .query_map(
                    named_params! {
                        ":service": params.service,
                        ":t_min": params.start,
                        ":t_max": params.end,
                        ":d_min": params.duration_min.map(|d| d.whole_microseconds() as u64),
                        ":d_max": params.duration_max.map(|d| d.whole_microseconds() as u64),
                        ":limit": params.limit,
                    },
                    |row| row.get::<_, [u8; 16]>(0),
                )?
                .map(|raw| TraceId::try_from(raw?).map(Into::into))
                .collect::<Result<Vec<Value>>>()
                .context("failed listing trace IDs")?;

            conn.prepare(include_str!("queries/list_spans.sql"))?
                .query_map([Rc::new(trace_ids)], |row| row.get(0))?
                .try_fold(HashMap::<TraceId, Vec<Span>>::new(), |mut map, entry| {
                    let span = decode_span(entry?).context("failed decoding span")?;

                    if span_contains_tag(&span, &params.tags) {
                        map.entry(span.trace_id).or_default().push(span);
                    }

                    anyhow::Ok(map)
                })
                .context("failed listing spans")
        })
        .await
    }

    #[instrument(skip_all)]
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

    #[instrument(skip_all)]
    pub async fn find_traces(
        &self,
        trace_ids: impl Iterator<Item = TraceId>,
    ) -> Result<HashMap<TraceId, Vec<Span>>> {
        let trace_ids = trace_ids.map(Into::into).collect::<Vec<Value>>();

        self.interact::<_, _, anyhow::Error>(move |conn| {
            conn.prepare(include_str!("queries/find_traces.sql"))?
                .query_map([Rc::new(trace_ids)], |row| row.get(0))?
                .try_fold(HashMap::<TraceId, Vec<Span>>::new(), |mut map, entry| {
                    let span = decode_span(entry?)?;
                    map.entry(span.trace_id).or_default().push(span);
                    Ok(map)
                })
        })
        .await
    }
}

fn encode_span(span: &Span) -> Result<Vec<u8>> {
    let buf = postcard::to_stdvec(span)?;
    let buf = zstd::bulk::compress(&*buf, 11)?;

    Ok(buf)
}

fn decode_span(span: Vec<u8>) -> Result<Span> {
    let span = zstd::decode_all(&*span)?;
    let span = postcard::from_bytes(&span)?;

    Ok(span)
}

struct TraceInfo {
    service: String,
    timestamp: OffsetDateTime,
    min_duration: Duration,
    max_duration: Duration,
}

impl TraceInfo {
    fn from_spans(spans: &[Span]) -> HashMap<TraceId, Self> {
        let mut map = HashMap::new();

        for span in spans {
            let info = map.entry(span.trace_id).or_insert_with(|| TraceInfo {
                service: span.process.service.clone(),
                timestamp: span.start,
                min_duration: span.duration,
                max_duration: span.duration,
            });

            info.timestamp = info.timestamp.min(span.start);
            info.min_duration = info.min_duration.min(span.duration);
            info.max_duration = info.max_duration.max(span.duration);
        }

        map
    }
}

#[derive(Debug)]
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
