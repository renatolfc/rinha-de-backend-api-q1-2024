FROM rust as builder

WORKDIR /usr/src/rinha

RUN wget https://musl.cc/x86_64-linux-musl-native.tgz && \
    tar xvf x86_64-linux-musl-native.tgz -C /usr/local && \
    rm x86_64-linux-musl-native.tgz && \
    ln -s /usr/local/x86_64-linux-musl-native/bin/x86_64-linux-musl-gcc /usr/local/bin/musl-gcc

RUN rustup target add x86_64-unknown-linux-musl

COPY . .
RUN \
    --mount=type=cache,target=/usr/src/rinha/target \
    env RUSTCFLAGS="-C target-feature=+avx2 -C target-feature=+fma -C target-feature=+ssse3 -C target-cpu=x86-64-v3" \
    cargo install --target=x86_64-unknown-linux-musl --path .

FROM gcr.io/distroless/static-debian12

COPY --from=builder /usr/local/cargo/bin /app

EXPOSE 9999

CMD ["/app/rinha", "-d", "postgres://postgres:postgres@172.17.0.2/rinha"]
