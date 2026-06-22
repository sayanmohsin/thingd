FROM node:24-bookworm-slim AS runtime

WORKDIR /app

ENV NODE_ENV=production
ENV THINGD_PATH=/data/thingd.db
ENV THINGD_DRIVER=native
ENV THINGD_HOST=0.0.0.0
ENV THINGD_PORT=8757
ENV THINGD_CLUSTER_MODE=single
ENV THINGD_CLUSTER_DISCOVERY=none
ENV THINGD_CLUSTER_PORT=8757
ENV THINGD_MCP_AUDIT=true

# Install pnpm
RUN corepack enable && corepack prepare pnpm@11.1.3 --activate

# Copy dependency manifests first (layer caching)
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY Cargo.toml Cargo.lock rustfmt.toml ./

# Install npm dependencies (cached unless lockfile changes)
RUN pnpm install --frozen-lockfile

# Copy all source code
COPY crates ./crates
COPY packages ./packages
COPY examples ./examples

# Pre-seed prebuilt native binaries (provided by CI build-native job)
# These are downloaded into packages/thingd-native/prebuilds/ before docker build
# If not present, the build will need Rust toolchain
RUN if [ -d packages/thingd-native/prebuilds ] && [ "$(ls -A packages/thingd-native/prebuilds 2>/dev/null)" ]; then \
      echo "Using prebuilt native binaries"; \
      for dir in packages/thingd-native/prebuilds/*/; do \
        platform=$(basename "$dir"); \
        mkdir -p packages/thingd-native/dist; \
        if [ -f "$dir/thingd_native.node" ]; then \
          cp "$dir/thingd_native.node" packages/thingd-native/dist/thingd_native.node; \
          echo "  Copied prebuilt for $platform"; \
          break; \
        fi; \
      done; \
    else \
      echo "No prebuilt native binaries found — will compile from source"; \
    fi

# Build all packages (TypeScript only — Rust is prebuilt or provided via deps)
RUN pnpm install --frozen-lockfile --offline 2>/dev/null; \
    pnpm --filter @thingd/sdk --filter @thingd/cli build

# Runtime setup
RUN mkdir -p /data

EXPOSE 8757
VOLUME ["/data"]

CMD ["node", "packages/thingd-cli/dist/index.js", "mcp-http"]
