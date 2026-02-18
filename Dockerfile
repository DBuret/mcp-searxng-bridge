# --- Étape 1 : Build ---
FROM rust:1.85-slim AS builder

# Installation de musl-tools pour avoir musl-gcc
RUN apt-get update && apt-get install -y \
    musl-tools \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# On s'assure que Rust utilise rustls pour éviter de dépendre de l'OpenSSL système
# (Vérifie bien ton Cargo.toml comme mentionné plus bas)
RUN rustup target add x86_64-unknown-linux-musl
RUN cargo build --release --target x86_64-unknown-linux-musl

# --- Étape 2 : Runtime final ---
FROM scratch

# --- LABEL OCI STANDARDS ---
# Informations de base
LABEL org.opencontainers.image.title="MCP SearXNG Rust Bridge"
LABEL org.opencontainers.image.description="High-performance MCP server bridge connecting AI agents to SearXNG via SSE. Features web search and smart Markdown scraping."
LABEL org.opencontainers.image.vendor="DBuret"
LABEL org.opencontainers.image.authors="DBuret"

# Liens et Documentation
LABEL org.opencontainers.image.url="https://github.com/votre-user/mcp-searxng-rs"
LABEL org.opencontainers.image.source="https://github.com/DBuret/mcp-searxng-bridge"
LABEL org.opencontainers.image.documentation="https://github.com/DBuret/mcp-searxng-bridge/blob/main/README.adoc"

# Versioning (à mettre à jour à chaque release)
LABEL org.opencontainers.image.version="0.3.1"
LABEL org.opencontainers.image.revision="7bae13f" 

# Licensing
LABEL org.opencontainers.image.licenses="MIT"

# Spécificités Runtime
LABEL com.paitrimony.mcp.protocol_version="2024-11-05"
LABEL com.paitrimony.mcp.transport="sse"
LABEL com.paitrimony.mcp.tools="search,fetch_page"


# On récupère uniquement les certificats SSL depuis le builder
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# On récupère notre binaire compilé statiquement
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/mcp-searxng-rs /app/mcp-bridge

# Variables d'environnement par défaut
ENV MCP_SX_URL="http://172.17.0.1:18080"
ENV MCP_SX_PORT="3000"
ENV MCP_SX_LOG="info"

WORKDIR /app
EXPOSE 3000
USER 1000

ENTRYPOINT ["./mcp-bridge"]