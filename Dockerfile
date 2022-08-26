FROM node:16-alpine as uibuilder

WORKDIR /volume

COPY archer-ui/ ./

RUN yarn install && yarn run build

FROM rust:1.63-alpine as chef

RUN apk add --no-cache musl-dev=~1.2 && \
    cargo install cargo-chef

WORKDIR /volume

FROM chef as planner

COPY ./ ./

RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder

RUN apk add --no-cache protobuf=~3.18 protobuf-dev=~3.18 thrift=~0.16

COPY --from=planner /volume/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY ./ ./
COPY --from=uibuilder /volume/packages/jaeger-ui/build/ /volume/archer-ui/packages/jaeger-ui/build/

RUN cargo build --release

FROM alpine:3.16 as newuser

RUN echo "archer:x:1000:" > /tmp/group && \
    echo "archer:x:1000:1000::/dev/null:/sbin/nologin" > /tmp/passwd

FROM scratch

COPY --from=builder /volume/target/release/archer /bin/
COPY --from=newuser /tmp/group /tmp/passwd /etc/

EXPOSE 6831 6832
EXPOSE 14250 14268
EXPOSE 16686
USER archer

ENTRYPOINT ["/bin/archer"]
