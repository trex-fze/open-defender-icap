FROM rust:1.76 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/icap-adaptor /usr/local/bin/icap-adaptor
COPY --from=builder /app/target/release/policy-engine /usr/local/bin/policy-engine
COPY --from=builder /app/target/release/llm-worker /usr/local/bin/llm-worker
COPY --from=builder /app/target/release/reclass-worker /usr/local/bin/reclass-worker
COPY --from=builder /app/target/release/admin-api /usr/local/bin/admin-api
COPY --from=builder /app/target/release/odctl /usr/local/bin/odctl
COPY config /app/config
ENTRYPOINT ["/bin/bash", "-c", "echo specify command"]
