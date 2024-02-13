FROM rust as builder

WORKDIR /usr/src/rinha
COPY . .
RUN cargo install --path .
RUN cargo install sqlx-cli

FROM rust:slim
COPY --from=builder /usr/local/cargo/bin /usr/local/cargo/bin
