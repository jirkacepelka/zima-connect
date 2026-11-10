# Build stage
FROM golang:1.22-alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache git ca-certificates tzdata

# Download dependencies
COPY go.mod go.sum* ./
RUN go mod download 2>/dev/null || true

# Copy source code
COPY . .

# Ensure dependencies are tidy and build static binary (CGO-free with modernc.org/sqlite)
RUN go mod tidy && CGO_ENABLED=0 GOOS=linux go build -ldflags="-s -w" -o /zima-connect-server ./cmd/zima-connect-server

# Final lightweight production image
FROM alpine:3.19

RUN apk --no-cache add ca-certificates tzdata

WORKDIR /app

COPY --from=builder /zima-connect-server /app/zima-connect-server

# Standard directories for ZimaOS
RUN mkdir -p /app/data /app/data/staging

ENV ZC_PORT=9120 \
    ZC_DASHBOARD_PORT=9121 \
    ZC_DATA_DIR=/app/data \
    ZC_LOG_LEVEL=info

EXPOSE 9120 9121

VOLUME ["/app/data"]

ENTRYPOINT ["/app/zima-connect-server"]
