FROM rust as builder

WORKDIR /usr/src/rinha
COPY . .

RUN wget https://musl.cc/x86_64-linux-musl-native.tgz && \
    tar xvf x86_64-linux-musl-native.tgz -C /usr/local && \
    rm x86_64-linux-musl-native.tgz && \
    ln -s /usr/local/x86_64-linux-musl-native/bin/x86_64-linux-musl-gcc /usr/local/bin/musl-gcc

RUN rustup target add x86_64-unknown-linux-musl
RUN cargo install --target=x86_64-unknown-linux-musl --path .

FROM alpine

COPY --from=builder /usr/local/cargo/bin /usr/local/cargo/bin
