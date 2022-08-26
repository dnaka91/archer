use std::net::Ipv4Addr;

const ADDRESS: Ipv4Addr = if cfg!(debug_assertions) {
    Ipv4Addr::LOCALHOST
} else {
    Ipv4Addr::UNSPECIFIED
};

pub const JAEGER_AGENT_COMPACT: (Ipv4Addr, u16) = (ADDRESS, 6831);
pub const JAEGER_AGENT_BINARY: (Ipv4Addr, u16) = (ADDRESS, 6832);
pub const JAEGER_COLLECTOR_GRPC: (Ipv4Addr, u16) = (ADDRESS, 14250);
pub const JAEGER_COLLECTOR_HTTP: (Ipv4Addr, u16) = (ADDRESS, 14268);
pub const JAEGER_QUERY_HTTP: (Ipv4Addr, u16) = (ADDRESS, 16686);

pub const OTLP_COLLECTOR_GRPC: (Ipv4Addr, u16) = (ADDRESS, 4317);
pub const OTLP_COLLECTOR_HTTP: (Ipv4Addr, u16) = (ADDRESS, 4318);
