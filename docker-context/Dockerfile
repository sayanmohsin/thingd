# thingd-server — static binary Docker image
# Binary is prebuilt on the CI runner and copied into this directory.

FROM scratch

COPY thingd-server /thingd-server
COPY config.example.yaml /etc/thingd/config.yaml

EXPOSE 8757
VOLUME ["/data"]

ENTRYPOINT ["/thingd-server"]
CMD ["--config", "/etc/thingd/config.yaml"]
