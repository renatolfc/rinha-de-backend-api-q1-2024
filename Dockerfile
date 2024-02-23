FROM rust as builder

WORKDIR /usr/src/rinha

RUN wget https://musl.cc/i686-linux-musl-native.tgz && \
    tar xvf i686-linux-musl-native.tgz -C /usr/local && \
    rm i686-linux-musl-native.tgz && \
    ln -s /usr/local/i686-linux-musl-native/bin/i686-linux-musl-gcc /usr/local/bin/musl-gcc

RUN rustup target add i686-unknown-linux-musl

COPY . .

RUN \
    --mount=type=cache,target=/usr/src/rinha/target \
    cargo build --target=i686-unknown-linux-musl --release
RUN \
    --mount=type=cache,target=/usr/src/rinha/target \
    cargo install --target=i686-unknown-linux-musl --path .

FROM gcr.io/distroless/static-debian12

COPY --from=builder /usr/local/cargo/bin /app

EXPOSE 9999

CMD ["/app/rinha", "-d", "postgres://postgres:postgres@172.17.0.2/rinha"]
