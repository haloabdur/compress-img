FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake nasm && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/target/release/compress-img /usr/local/bin/compress-img
ENV BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["compress-img"]
