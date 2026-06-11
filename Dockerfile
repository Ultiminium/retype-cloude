FROM rust:1.76-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/retype-cloud /usr/local/bin/retype-cloud
EXPOSE 3000
CMD ["retype-cloud"]
