FROM node:22-bookworm AS build

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential ca-certificates curl pkg-config \
  && rm -rf /var/lib/apt/lists/*

ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal

RUN corepack enable && corepack prepare pnpm@11.1.3 --activate

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates ./crates
COPY packages ./packages
COPY examples ./examples

RUN pnpm install --frozen-lockfile
RUN pnpm build

FROM node:22-bookworm-slim AS runtime

WORKDIR /app

ENV NODE_ENV=production
ENV MEMORYD_PATH=/data/memoryd.db
ENV MEMORYD_DRIVER=native
ENV MEMORYD_HOST=0.0.0.0
ENV MEMORYD_PORT=8757
ENV MEMORYD_CLUSTER_MODE=single
ENV MEMORYD_CLUSTER_DISCOVERY=none
ENV MEMORYD_CLUSTER_PORT=8757
ENV MEMORYD_MCP_AUDIT=true

COPY --from=build /app/package.json /app/pnpm-lock.yaml /app/pnpm-workspace.yaml ./
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/packages ./packages

RUN mkdir -p /data

EXPOSE 8757
VOLUME ["/data"]

CMD ["node", "packages/memoryd-mcp/dist/http-cli.js"]
