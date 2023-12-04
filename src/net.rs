use std::net::Ipv4Addr;

const ADDRESS: Ipv4Addr = if cfg!(debug_assertions) {
    Ipv4Addr::LOCALHOST
} else {
    Ipv4Addr::UNSPECIFIED
};

pub const JAEGER_QUERY_HTTP: (Ipv4Addr, u16) = (ADDRESS, 16686);

pub const OTLP_COLLECTOR_GRPC: (Ipv4Addr, u16) = (ADDRESS, 4317);
pub const OTLP_COLLECTOR_HTTP: (Ipv4Addr, u16) = (ADDRESS, 4318);

pub const QUIVER_COLLECTOR: (Ipv4Addr, u16) = (ADDRESS, 14000);
