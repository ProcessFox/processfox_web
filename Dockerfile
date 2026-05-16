# ProcessFox Web — Multi-Stage Build (CLAUDE.md §12).
# Build-Kontext = Repo-Root.

# ---- Stage 1: Frontend (Vite) -------------------------------------------
FROM node:22-alpine AS frontend
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY index.html vite.config.ts tsconfig.json tsconfig.node.json ./
COPY tailwind.config.js postcss.config.js components.json ./
COPY public ./public
COPY src ./src
RUN npm run build

# ---- Stage 2: Backend (Rust/Axum) ---------------------------------------
FROM rust:1-bookworm AS backend
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock ./
# Dummy-main, damit der Dependency-Layer separat (und reproduzierbar via
# --locked) cachen kann.
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked 2>/dev/null || true
COPY backend/src ./src
# mtime aktualisieren, sonst baut cargo den echten main.rs nicht neu.
RUN touch src/main.rs && cargo build --release --locked

# ---- Stage 3: Runtime ---------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=frontend /app/dist /app/static
COPY --from=backend /app/target/release/processfox-web /app/processfox-web
ENV STATIC_DIR=/app/static
ENV PORT=3000
EXPOSE 3000
CMD ["/app/processfox-web"]
