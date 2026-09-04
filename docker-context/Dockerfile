# thingd-server — static binary Docker image
# Binary is prebuilt on the CI runner and copied into this directory.
# TARGETARCH is set automatically by docker buildx --platform.

# reqwest's Rustls platform verifier needs the system CA bundle when the
# runtime fetches HTTPS connector sources. Keep the final image scratch-based,
# but copy the bundle from a small build stage.
FROM alpine:3.22 AS ca-certificates
RUN mkdir -p /tmp

FROM scratch

ARG TARGETARCH
COPY --from=ca-certificates /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=ca-certificates /tmp /tmp
COPY --chmod=755 thingd-server-${TARGETARCH} /thingd-server
COPY config.example.yaml /etc/thingd/config.yaml

EXPOSE 8757
VOLUME ["/data"]

ENTRYPOINT ["/thingd-server"]
CMD ["--config", "/etc/thingd/config.yaml"]
