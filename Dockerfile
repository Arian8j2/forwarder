FROM rust:1.79-alpine3.20 as builder

RUN rustup target add x86_64-unknown-linux-musl
RUN apk add --no-cache musl-dev git

WORKDIR /app
COPY . .
RUN cargo build --release --package forwarder-cli --target x86_64-unknown-linux-musl

FROM alpine:3.20 as runtime
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/forwarder-cli forwarder

ENV LISTEN_ADDR=0.0.0.0:1001 \
    REDIRECT_ADDR=127.0.0.1:8585 \
    PASSPHRASE=haha
CMD ./forwarder -l $LISTEN_ADDR -r $REDIRECT_ADDR -p $PASSPHRASE $OTHER_ARGS
