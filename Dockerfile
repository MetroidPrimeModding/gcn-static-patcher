FROM --platform=$BUILDPLATFORM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
  && rm -rf /var/lib/apt/lists/*

ARG TARGETARCH
COPY dist/gcn-static-patcher-cli-${TARGETARCH} /usr/local/bin/gcn-static-patcher-cli
RUN chmod +x /usr/local/bin/gcn-static-patcher-cli

ENTRYPOINT ["/usr/local/bin/gcn-static-patcher-cli"]
