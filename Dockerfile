FROM rust:trixie AS builder
WORKDIR /usr/src/ypb
COPY . .
RUN cargo install --path .

FROM debian:trixie-slim
#RUN apt-get update && apt-get install -y openssl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ypb /usr/local/bin/ypb
CMD ["ypb"]
