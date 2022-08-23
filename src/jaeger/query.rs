use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    net::{Ipv4Addr, SocketAddr},
    num::NonZeroU128,
};

use anyhow::Result;
use archer_http::{
    axum::{
        extract::{
            rejection::{FailedToDeserializeQueryString, QueryRejection},
            Path, Query,
        },
        headers::IfNoneMatch,
        http::{
            header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, LAST_MODIFIED},
            HeaderMap, HeaderValue, StatusCode, Uri,
        },
        response::IntoResponse,
        routing::get,
        Extension, Router, Server, TypedHeader,
    },
    tower::ServiceBuilder,
    tower_http::ServiceBuilderExt,
    ApiError, ApiResponse, TraceId,
};
use serde::Deserialize;
use time::Duration;
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

use crate::{convert, storage::Database};

#[instrument(name = "query", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let app = Router::new()
        .route("/api/services", get(services))
        .route("/api/services/:service/operations", get(operations))
        .route("/api/operations", get(todo))
        .route("/api/traces", get(traces))
        .route("/api/traces/:id", get(trace))
        .route("/api/archive/:id", get(todo))
        .route("/api/dependencies", get(dependencies))
        .route("/api/metrics/latencies", get(todo))
        .route("/api/metrics/calls", get(todo))
        .route("/api/metrics/errors", get(todo))
        .route("/api/metrics/minstep", get(todo))
        .fallback(get(asset))
        .layer(
            ServiceBuilder::new()
                .trace_for_http()
                .layer(Extension(database)),
        );

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 16686));
    info!("listening on http://{addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

async fn services(Extension(db): Extension<Database>) -> impl IntoResponse {
    ApiResponse::Data(db.list_services().await.unwrap())
}

async fn operations(
    Path(service): Path<String>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    ApiResponse::Data(db.list_operations(service).await.unwrap())
}

#[cfg_attr(test, derive(Debug, PartialEq))]
struct TraceIdsQuery(Vec<TraceId>);

impl<'de> Deserialize<'de> for TraceIdsQuery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TraceIdsQuery;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("trace IDs as map with all keys having the same value")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut ids = Vec::new();

                while let Some((k, v)) = access.next_entry::<Cow<'_, str>, Cow<'_, str>>()? {
                    if k != "traceID" {
                        return Err(serde::de::Error::custom("unknown key"));
                    }

                    ids.push(v.parse().map_err(serde::de::Error::custom)?);
                }

                if ids.is_empty() {
                    return Err(serde::de::Error::custom("no trace IDs"));
                }

                Ok(TraceIdsQuery(ids))
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

#[cfg_attr(test, derive(Debug, Default, PartialEq))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TracesQuery {
    service: String,
    operation: Option<String>,
    #[serde(default, deserialize_with = "de::duration_micros")]
    start: Option<Duration>,
    #[serde(default, deserialize_with = "de::duration_micros")]
    end: Option<Duration>,
    #[serde(default, deserialize_with = "de::duration_human")]
    min_duration: Option<Duration>,
    #[serde(default, deserialize_with = "de::duration_human")]
    max_duration: Option<Duration>,
    #[serde(default, deserialize_with = "de::limit")]
    limit: Option<u32>,
    #[serde(default, flatten, deserialize_with = "de::tags")]
    tags: HashMap<String, String>,
}

mod de {
    use std::{borrow::Cow, collections::HashMap, fmt, ops::Neg};

    use anyhow::{bail, ensure, Result};
    use serde::{
        de::{self, Visitor},
        Deserializer,
    };
    use time::Duration;

    pub fn tags<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(TagsVisitor)
    }

    struct TagsVisitor;

    impl<'de> Visitor<'de> for TagsVisitor {
        type Value = HashMap<String, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("tags as single <key>:<value> pair or JSON map")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut map = HashMap::new();

            while let Some((k, v)) = access.next_entry::<Cow<'_, str>, Cow<'_, str>>()? {
                match &*k {
                    "tag" => {
                        let (k, v) = v
                            .split_once(':')
                            .ok_or_else(|| de::Error::custom("missing `:` separator"))?;

                        map.insert(k.to_owned(), v.to_owned());
                    }
                    "tags" => {
                        let kvs = serde_json::from_str::<HashMap<_, _>>(&*v)
                            .map_err(|e| de::Error::custom(format!("invalid JSON map: {e}")))?;

                        map.extend(kvs);
                    }
                    _ => continue,
                }
            }

            Ok(map)
        }
    }

    pub fn duration_micros<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_option(DurationMicrosVisitor)
    }

    struct DurationMicrosVisitor;

    impl<'de> Visitor<'de> for DurationMicrosVisitor {
        type Value = Option<Duration>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("duration in milliseconds")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(Duration::microseconds(v)))
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            v.parse::<i64>()
                .map_err(E::custom)
                .and_then(|v| self.visit_i64(v))
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_i64(Self)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }
    }

    pub fn duration_human<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_option(DurationHumanVisitor)
    }

    struct DurationHumanVisitor;

    impl DurationHumanVisitor {
        fn parse(value: &str) -> Result<Duration> {
            // Check for negative duration, denoted by a leading `-` sign.
            let (mut value, negative) = match value.strip_prefix('-') {
                Some(v) => (v, true),
                None => (value, false),
            };

            ensure!(
                value.starts_with(|c: char| c.is_ascii_digit()),
                "must start with a digit"
            );

            let mut total = Duration::ZERO;

            while let Some((start, end)) = Self::find_next_unit(value) {
                let number = value[..start].parse::<f64>()?;
                let duration = match &value[start..end] {
                    "ns" => Duration::nanoseconds(number.floor() as _),
                    "us" | "µs" => Duration::nanoseconds((number * 1_000.0).floor() as _),
                    "ms" => Duration::nanoseconds((number * 1_000_000.0).floor() as _),
                    "s" => Duration::seconds_f64(number),
                    "m" => Duration::seconds_f64(number * 60.0),
                    "h" => Duration::seconds_f64(number * 3600.0),
                    v => bail!("invalid unit: {v}"),
                };

                total += duration;
                value = &value[end..];
            }

            ensure!(value.is_empty(), "unexpected trailing data: {value}");

            Ok(negative.then(|| total.neg()).unwrap_or(total))
        }

        fn find_next_unit(value: &str) -> Option<(usize, usize)> {
            let find_start = |value: &str| value.find(|c: char| c.is_ascii_alphabetic());
            let find_end = |value: &str, start: usize| {
                value[start..]
                    .find(|c: char| c.is_ascii_digit())
                    .map(|end| start + end)
                    .unwrap_or(value.len())
            };

            find_start(value).map(|start| (start, find_end(value, start)))
        }
    }

    impl<'de> Visitor<'de> for DurationHumanVisitor {
        type Value = Option<Duration>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("duration in human readable form")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(None)
            } else {
                Self::parse(v).map(Some).map_err(E::custom)
            }
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(Self)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }
    }

    pub fn limit<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_option(LimitVisitor)
    }

    struct LimitVisitor;

    impl<'de> Visitor<'de> for LimitVisitor {
        type Value = Option<u32>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("trace limit as integer")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if v.is_empty() {
                Ok(None)
            } else {
                v.parse().map(Some).map_err(E::custom)
            }
        }

        fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(v))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            u32::try_from(v)
                .map_err(E::custom)
                .and_then(|v| self.visit_u32(v))
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            u32::try_from(v)
                .map_err(E::custom)
                .and_then(|v| self.visit_u32(v))
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_u32(Self)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deser_trace_ids() {
        let expect = TraceIdsQuery(vec![TraceId(5), TraceId(6)]);
        let result = serde_urlencoded::from_str(&format!("traceID={:032x}&traceID={:032x}", 5, 6));

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_basic() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str("service=test");

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_tags_tuple() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            tags: [("a".to_owned(), "1".to_owned())].into_iter().collect(),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str("service=test&tag=a:1");

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_tags_json() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            tags: [("a".to_owned(), "1".to_owned())].into_iter().collect(),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str(r#"service=test&tags={"a":"1"}"#);

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_limit() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            limit: Some(5),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str("service=test&limit=5");

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_durations() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            min_duration: Some(Duration::milliseconds(10)),
            max_duration: Some(
                Duration::hours(1)
                    + Duration::minutes(12)
                    + Duration::minutes(30)
                    + Duration::seconds(45)
                    + Duration::milliseconds(120)
                    + Duration::microseconds(200),
            ),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str(
            "\
            service=test&\
            minDuration=10ms&\
            maxDuration=1.2h30m45s120.2ms\
            ",
        );

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_all() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            operation: Some("op1".to_owned()),
            start: Some(Duration::microseconds(1661232631416000)),
            end: Some(Duration::microseconds(1661236231416000)),
            limit: Some(20),
            tags: [
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "2".to_owned()),
            ]
            .into_iter()
            .collect(),
            ..TracesQuery::default()
        };
        // end=1661236231416000&limit=20&lookback=1h&maxDuration&minDuration&service=twitchid&start=1661232631416000
        let result = serde_urlencoded::from_str(
            "\
            service=test&\
            operation=op1&\
            start=1661232631416000&\
            end=1661236231416000&\
            minDuration&\
            maxDuration&\
            lookback=1h&\
            tag=a:1&\
            limit=20&\
            tags={\"b\":\"2\"}\
            ",
        );

        assert_eq!(expect, result.unwrap());
    }
}

async fn traces(
    query: Result<Query<TracesQuery>, QueryRejection>,
    trace_ids: Result<Query<TraceIdsQuery>, QueryRejection>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    let spans = match (query, trace_ids) {
        (Ok(Query(query)), Err(_)) => db.list_spans(query.service, query.operation).await.unwrap(),
        (Err(_), Ok(Query(ids))) => {
            let spans = db
                .find_traces(
                    ids.0
                        .iter()
                        .map(|id| NonZeroU128::new(id.0).unwrap().into()),
                )
                .await
                .unwrap();

            if spans.is_empty() {
                return ApiResponse::Error(ApiError {
                    code: StatusCode::NOT_FOUND,
                    msg: "trace id not found".into(),
                    trace_id: ids.0.get(0).copied(),
                });
            }

            spans
        }
        (Ok(_), Ok(_)) => {
            return ApiResponse::Error(ApiError {
                code: StatusCode::BAD_REQUEST,
                msg: "can't search by trace IDs and query at the same time".into(),
                trace_id: None,
            });
        }
        (Err(e), Err(_)) => {
            return ApiResponse::Error(ApiError {
                code: StatusCode::BAD_REQUEST,
                msg: e.to_string().into(),
                trace_id: None,
            });
        }
    };

    let traces = spans
        .into_iter()
        .map(|(trace_id, spans)| convert::trace_to_json(trace_id, spans))
        .collect();

    ApiResponse::Data(traces)
}

async fn trace(
    Path(trace_id): Path<TraceId>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    let spans = db
        .find_trace(NonZeroU128::new(trace_id.0).unwrap().into())
        .await
        .unwrap();
    let trace = match spans.get(0) {
        Some(span) => convert::trace_to_json(span.trace_id, spans),
        None => {
            return ApiResponse::Error(ApiError {
                code: StatusCode::NOT_FOUND,
                msg: "trace ID not found".into(),
                trace_id: None,
            })
        }
    };

    ApiResponse::Data(vec![trace])
}

async fn dependencies() -> impl IntoResponse {
    ApiResponse::Data(Vec::<()>::new())
}

async fn todo() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

async fn asset(uri: Uri, if_none_match: Option<TypedHeader<IfNoneMatch>>) -> impl IntoResponse {
    let asset = ASSETS
        .get(uri.path())
        .or_else(|| ASSETS.get("/index.html"))
        .ok_or_else(|| (HeaderMap::new(), StatusCode::NOT_FOUND))?;

    let headers = [
        (CONTENT_TYPE, asset.mime),
        (ETAG, asset.etag),
        (LAST_MODIFIED, "Thu, 01 Jan 1970 00:00:00 GMT"),
        (CACHE_CONTROL, "public, max-age=2592000, must-revalidate"),
    ]
    .into_iter()
    .map(|(name, value)| (name, HeaderValue::from_static(value)))
    .collect::<HeaderMap>();

    let unmatched = if_none_match
        .map(|v| v.precondition_passes(&asset.etag.parse().unwrap()))
        .unwrap_or(true);

    if unmatched {
        Ok((headers, asset.content))
    } else {
        Err((headers, StatusCode::NOT_MODIFIED))
    }
}
