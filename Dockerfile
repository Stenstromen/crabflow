FROM rust:alpine AS builder
WORKDIR /app
COPY . .
RUN apk add --no-cache libressl-dev musl-dev gcc && \
    rustup target add x86_64-unknown-linux-musl && \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=gcc cargo build --target x86_64-unknown-linux-musl --release

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/crabflow /crabflow
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
USER 65534:65534
CMD ["/crabflow"]