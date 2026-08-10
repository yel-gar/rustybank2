FROM rust:1-alpine3.24 AS builder
WORKDIR /app/build

COPY Cargo.toml Cargo.lock ./
COPY .sqlx/ .sqlx/
COPY src/ src/

ENV SQLX_OFFLINE=1
RUN cargo install --path .

FROM alpine:3.24 AS runner
WORKDIR /app
ARG FLAG
ENV DB_FILE=/app/data/db.sqlite

RUN apk add --no-cache sqlite

COPY res/ res/
RUN chmod +x ./res/init.sh
RUN ./res/init.sh

COPY --from=builder /usr/local/cargo/bin/rustybank2 /usr/local/bin/rustybank2

ENV RUST_LOG="rustybank2=info"
CMD ["rustybank2"]
