FROM --platform=$BUILDPLATFORM rust:latest AS build
ARG TARGETPLATFORM
RUN apt-get update && apt-get install -y cmake
WORKDIR /app
COPY . .
RUN case "$TARGETPLATFORM" in \
        linux/amd64) TARGET="x86_64-unknown-linux-gnu" ;; \
        linux/arm64) TARGET="aarch64-unknown-linux-gnu" ;; \
    esac && \
    rustup target add "$TARGET" && \
    cargo build -p thingd-server --release --target "$TARGET" && \
    cp "target/$TARGET/release/thingd-server" /thingd-server

FROM scratch
COPY --from=build /thingd-server /thingd-server
EXPOSE 8757
VOLUME ["/data"]
ENTRYPOINT ["/thingd-server"]
