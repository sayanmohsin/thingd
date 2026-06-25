import { defineConfig } from "vitepress";

export default defineConfig({
  title: "thingd",
  description: "A fast object-first data engine for applications and AI agents",
  base: "/thingd/",
  ignoreDeadLinks: true,

  head: [
    ["link", { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" }],
    ["meta", { name: "theme-color", content: "#07090b" }],
  ],

  themeConfig: {
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
        ],
      },
    ],

    sidebar: [
      {
        text: "Getting Started",
        items: [
          { text: "Quick Start", link: "/quickstart" },
          { text: "FAQ", link: "/faq" },
          { text: "Release Notes", link: "/release" },
        ],
      },
      {
        text: "API Reference",
        items: [
          { text: "Overview", link: "/api-spec/" },
          { text: "REST API", link: "/api-spec/rest-api" },
          { text: "Data Model", link: "/api-spec/data-model" },
          { text: "MCP Tools", link: "/api-spec/mcp-tools" },
          { text: "Search", link: "/api-spec/search" },
          { text: "Errors", link: "/api-spec/errors" },
        ],
      },
      {
        text: "CLI",
        items: [{ text: "CLI Reference", link: "/cli-reference" }],
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
    ],

    socialLinks: [{ icon: "github", link: "https://github.com/sayanmohsin/thingd" }],

    footer: {
      message: "Built with Rust · Apache-2.0 License",
      copyright: "© 2024–present <a href='https://github.com/sayanmohsin'>Sayan Mohsin</a>",
    },
  },
});
