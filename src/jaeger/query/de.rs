use std::{borrow::Cow, collections::HashMap, fmt, ops::Neg};

use anyhow::{bail, ensure, Result};
use archer_http::TraceId;
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

pub fn trace_ids<'de, D>(deserializer: D) -> Result<Vec<TraceId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_map(TraceIdsVisitor)
}

struct TraceIdsVisitor;

impl<'de> serde::de::Visitor<'de> for TraceIdsVisitor {
    type Value = Vec<TraceId>;

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

        Ok(ids)
    }
}
