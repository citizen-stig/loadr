# Minimal loadr image for the Kubernetes manifests.
#   docker build -t your-registry/loadr:v1 examples/k8s
#   docker push your-registry/loadr:v1
# Then point the image: fields in *.yaml at your-registry/loadr:v1.
FROM debian:bookworm-slim
ARG LOADR_VERSION=latest
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN curl -sSL "https://github.com/levantar-ai/loadr/releases/${LOADR_VERSION}/download/loadr-x86_64-unknown-linux-gnu.tar.gz" \
      | tar xz -C /usr/local/bin loadr \
    && chmod +x /usr/local/bin/loadr
ENTRYPOINT ["loadr"]
