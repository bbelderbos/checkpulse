FROM rust:1.96-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd --create-home app && mkdir /data && chown app /data
COPY --from=builder /app/target/release/checkpulse /usr/local/bin/checkpulse
USER app
ENV BIND=0.0.0.0 PORT=8080 DATABASE_PATH=/data/checkpulse.db
EXPOSE 8080
CMD ["checkpulse"]
