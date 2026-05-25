FROM rust:1.95-slim AS chef

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends clang lld \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTFLAGS="-C link-arg=-fuse-ld=lld"

RUN cargo install cargo-chef --locked

FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --bin zero2prod --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin zero2prod

FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/zero2prod /usr/local/bin/zero2prod
COPY configuration ./configuration

ENV APP_ENVIRONMENT=docker

EXPOSE 8000

CMD ["zero2prod"]
