# syntax=docker/dockerfile:1.7
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN rustup component add rustfmt
RUN cargo fmt
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release \
    && cp /app/target/release/mdpdf-web /tmp/mdpdf-web

FROM node:22-bookworm-slim
ENV MDPDF_BIND=0.0.0.0:8080 \
    MDPDF_WORKDIR=/app/workdir \
    MDPDF_THEMES=/app/themes \
    MDPDF_CHROMIUM=chromium \
    MDPDF_PRINT_SCRIPT=/app/scripts/print_pdf.mjs \
    MDPDF_PUPPETEER_CONFIG=/app/puppeteer-config.json \
    PUPPETEER_SKIP_DOWNLOAD=true \
    PUPPETEER_EXECUTABLE_PATH=/usr/bin/chromium
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    chromium \
    fonts-noto-cjk \
    fonts-noto-color-emoji \
    && npm install -g @mermaid-js/mermaid-cli \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app


COPY --from=builder /tmp/mdpdf-web /usr/local/bin/mdpdf-web
COPY public ./public
COPY themes ./themes
COPY scripts ./scripts
COPY puppeteer-config.json ./puppeteer-config.json
RUN mkdir -p /app/workdir/files /app/workdir/jobs
EXPOSE 8080
CMD ["mdpdf-web"]
