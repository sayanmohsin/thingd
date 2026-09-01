import { readFileSync } from "node:fs";
import { defineConfig } from "vitepress";

const mcpToolCount = 49;
const packageVersion = JSON.parse(
  readFileSync(new URL("../../package.json", import.meta.url), "utf8")
).version as string;

export default defineConfig({
  title: "thingd — Open-Source Rust Data Engine for AI Agents",
  description: `thingd is an open-source Rust data engine for AI agents. Object-shaped storage, durable queues, event streams, full-text search, and ${mcpToolCount} MCP tools — all in one binary. Deploy via Docker, embed in Node.js, or run as a sidecar. Built by Sayan Mohsin.`,
  base: "/",
  ignoreDeadLinks: true,
  lang: "en-US",

  head: [
    ["link", { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" }],
    ["meta", { name: "theme-color", content: "#07090b" }],
    [
      "meta",
      { property: "og:title", content: "thingd — Open-Source Rust Data Engine for AI Agents" },
    ],
    [
      "meta",
      {
        property: "og:description",
        content: `Object-shaped storage, durable queues, event streams, full-text search, and ${mcpToolCount} MCP tools — all in one open-source Rust binary. Built by Sayan Mohsin.`,
      },
    ],
    ["meta", { name: "twitter:card", content: "summary_large_image" }],
    ["link", { rel: "canonical", href: "https://engine.thingd.cloud/" }],
  ],

  themeConfig: {
    mcpToolCount,
    packageVersion,
    logo: "/logo.svg",
    siteTitle: "",

    search: {
      provider: "local",
    },

    nav: [
      { text: "Getting Started", link: "/quickstart" },
      { text: "API Reference", link: "/api-spec/" },
      {
        text: "More",
        items: [
          { text: "CLI Reference", link: "/cli-reference" },
          { text: "Docker Setup", link: "/docker-hub" },
          { text: "MCP Server", link: "/mcp-server" },
          { text: "Agent Setup", link: "/agent-setup" },
          { text: "Operations", link: "/operations" },
          { text: "Storage Backends", link: "/storage-backends" },
          { text: "Security", link: "/security" },
          { text: "FAQ", link: "/faq" },
        ],
      },
      {
        text: "Links",
        items: [
          {
            text: "GitHub",
            link: "https://github.com/sayanmohsin/thingd",
          },
          {
            text: "npm (SDK)",
            link: "https://www.npmjs.com/package/@thingd/sdk",
          },
          {
            text: "Docker Hub",
            link: "https://hub.docker.com/r/sayanmohsin/thingd",
          },
          {
            text: "thingd Cloud",
            link: "https://thingd.cloud",
          },
        ],
      },
    ],

    sidebar: [
      {
        text: "Getting Started",
        items: [
          { text: "Quick Start", link: "/quickstart" },
          { text: "Schema Files", link: "/schema" },
          { text: "Why thingd?", link: "/why-thingd" },
          { text: "FAQ", link: "/faq" },
          { text: "Release Notes", link: "/release" },
        ],
      },
      {
        text: "Guides",
        items: [{ text: "Queue Deep Dive", link: "/guides/queues" }],
      },
      {
        text: "API Reference",
        items: [
          { text: "Overview", link: "/api-spec/" },
          { text: "REST API", link: "/api-spec/rest-api" },
          { text: "Data Model", link: "/api-spec/data-model" },
          { text: "MCP Tools", link: "/api-spec/mcp-tools" },
          { text: "Replication", link: "/api-spec/replication" },
          { text: "Search", link: "/api-spec/search" },
          { text: "Errors", link: "/api-spec/errors" },
        ],
      },
      {
        text: "CLI",
        items: [
          { text: "CLI Reference", link: "/cli-reference" },
          { text: "Schema Files", link: "/schema" },
        ],
      },
      {
        text: "Docker",
        items: [
          { text: "Docker Hub", link: "/docker-hub" },
          { text: "Docker Runtime", link: "/docker-runtime" },
        ],
      },
      {
        text: "MCP Server",
        items: [{ text: "MCP Server Setup", link: "/mcp-server" }],
      },
      {
        text: "Agent Guides",
        items: [
          { text: "Agent Setup", link: "/agent-setup" },
          { text: "Implementation Guide", link: "/agent-implementation-guide" },
          { text: "Nice Code Review", link: "/nice-code" },
          { text: "Agent Patterns", link: "/agent-patterns" },
          { text: "Why Agents?", link: "/why-agents" },
        ],
      },
      {
        text: "Production",
        items: [
          { text: "Operations", link: "/operations" },
          { text: "Security", link: "/security" },
          { text: "Runtime Environment", link: "/runtime-env" },
          { text: "Storage Backends", link: "/storage-backends" },
          { text: "Sidecar & Cluster", link: "/sidecar-cluster" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Architecture", link: "/architecture" },
          { text: "Benchmarks", link: "/benchmarks" },
        ],
      },
      {
        text: "thingd Cloud",
        link: "https://thingd.cloud",
      },
    ],

    socialLinks: [{ icon: "github", link: "https://github.com/sayanmohsin/thingd" }],

    footer: {
      message:
        "Built by <a href='https://sayanmohsin.com'>Sayan Mohsin</a> · Rust core · Apache-2.0",
      copyright:
        "© 2026 Sayan Mohsin. <a href='https://thingd.cloud'>thingd Cloud</a> — managed hosting for thingd.",
    },
  },
});
