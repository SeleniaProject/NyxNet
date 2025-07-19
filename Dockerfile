# syntax=docker/dockerfile:1.4

# -------- build stage --------
FROM --platform=$BUILDPLATFORM rust:1.77-slim AS builder
ARG TARGETPLATFORM
ARG BUILDPLATFORM
ARG TARGET=x86_64-unknown-linux-musl
RUN rustup target add $TARGET
WORKDIR /workspace
COPY . .
RUN cargo build --release --workspace --exclude nyx-sdk-wasm --target $TARGET

# -------- runtime stage --------
FROM gcr.io/distroless/cc-debian12@sha256:ce5c00e38acfc34b4e2cbbded4086985a45fc5517fcdba833ca6123e3aa8b6e1
COPY --from=builder /workspace/target/x86_64-unknown-linux-musl/release/nyxd /usr/bin/nyxd
ENTRYPOINT ["/usr/bin/nyxd"] 