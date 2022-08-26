FROM node:16-alpine as uibuilder

WORKDIR /volume

COPY archer-ui/ ./

RUN yarn install && yarn run build

FROM rust:1.63 as chef

RUN apt-get update && \
    apt-get install -y --no-install-recommends musl-tools=1.2.2-1 && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo install cargo-chef

WORKDIR /volume

FROM chef as planner

COPY ./ ./

RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder

RUN echo 'deb http://deb.debian.org/debian bookworm main' > /etc/apt/sources.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
    libprotobuf-dev=3.12.4-1+b4 \
    protobuf-compiler=3.12.4-1+b4 \
    thrift-compiler=0.16.0-5

COPY --from=planner /volume/recipe.json recipe.json

RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

COPY ./ ./
COPY --from=uibuilder /volume/packages/jaeger-ui/build/ /volume/archer-ui/packages/jaeger-ui/build/

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:3.16 as newuser

RUN echo "archer:x:1000:" > /tmp/group && \
    echo "archer:x:1000:1000::/dev/null:/sbin/nologin" > /tmp/passwd

FROM scratch

COPY --from=builder /volume/target/x86_64-unknown-linux-musl/release/archer /bin/
COPY --from=newuser /tmp/group /tmp/passwd /etc/

EXPOSE 6831 6832
EXPOSE 14250 14268
EXPOSE 16686
USER archer

ENTRYPOINT ["/bin/archer"]
