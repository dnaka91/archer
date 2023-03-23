FROM node:18-alpine as uibuilder

WORKDIR /volume

COPY archer-ui/ ./

RUN yarn install && yarn run build

FROM rust:1.68 as chef

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

RUN apt-get update && \
    apt-get install -y --no-install-recommends musl-tools && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo install cargo-chef

WORKDIR /volume

FROM chef as planner

COPY ./ ./

RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libprotobuf-dev \
    protobuf-compiler

COPY --from=planner /volume/recipe.json recipe.json

RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

COPY archer-http/ archer-http/
COPY archer-proto/ archer-proto/
COPY archer-thrift/ archer-thrift/
COPY archer-thrift-derive/ archer-thrift-derive/
COPY jaeger-idl/ jaeger-idl/
COPY opentelemetry-proto/ opentelemetry-proto/
COPY src/ src/
COPY build.rs Cargo.lock Cargo.toml ./

COPY --from=uibuilder /volume/packages/jaeger-ui/build/ archer-ui/packages/jaeger-ui/build/

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:3 as newuser

RUN echo "archer:x:1000:" > /tmp/group && \
    echo "archer:x:1000:1000::/dev/null:/sbin/nologin" > /tmp/passwd && \
    mkdir /var/lib/archer

FROM scratch

COPY --from=builder /volume/target/x86_64-unknown-linux-musl/release/archer /bin/
COPY --from=newuser /tmp/group /tmp/passwd /etc/
COPY --from=newuser --chown=1000 /var/lib/archer /var/lib/

# Jaeger Agent/Collector/Query ports
EXPOSE 6831 6832
EXPOSE 14250 14268
EXPOSE 16686
# OTLP Collector ports
EXPOSE 4317 4318
# Quiver Collector port
EXPOSE 14000/udp

USER archer

ENTRYPOINT ["/bin/archer"]
