# Build from the repository root:  docker build -f server/Dockerfile -t zima-connect-server .
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY core/ core/
RUN cd core && cargo build --release -p zima-connect-hub

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/core/target/release/zima-connect-hub /usr/local/bin/zima-connect-hub
ENV ZC_DATA_DIR=/data \
    ZC_DASHBOARD_PORT=9121
VOLUME ["/data"]
EXPOSE 9121
ENTRYPOINT ["zima-connect-hub"]
