import { once } from "node:events";
import { createServer } from "node:http";
import { expect, test } from "@playwright/test";
import { startDashboardServer } from "../../packages/thingd-cli/dist/dashboard/server.js";
import { handleRestRequest, ThingD } from "../../packages/thingd/dist/index.js";

test("dashboard login survives remote database switching and reloads", async ({ page }) => {
  const dashboardToken = "browser-smoke-dashboard-token";
  const remoteToken = "browser-smoke-remote-token";
  const remoteAuth = [];
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  const remoteDb = await ThingD.open({ path: ":memory:", driver: "memory" });
  const upstream = createServer(async (req, res) => {
    remoteAuth.push(req.headers.authorization);
    if (req.headers.authorization !== `Bearer ${remoteToken}`) {
      res.writeHead(401, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ error: "Remote token required" }));
      return;
    }
    await handleRestRequest(remoteDb, req, res, new URL(req.url, "http://localhost").pathname);
  });
  let dashboard;
  try {
    await remoteDb.put("smoke-remote", { id: "example", text: "Remote data" });
    upstream.listen(0, "127.0.0.1");
    await once(upstream, "listening");
    const remoteUrl = `http://127.0.0.1:${upstream.address().port}`;
    dashboard = await startDashboardServer(
      { path: ":memory:", driver: "memory", cloud: false, authToken: dashboardToken },
      0
    );
    const dashboardUrl = `http://127.0.0.1:${dashboard.server.address().port}`;
    const login = page.getByRole("heading", { name: "thingd Dashboard Access" });
    const active = page.getByText("Active (Localhost)", { exact: true });

    await page.goto(dashboardUrl);
    await expect(login).toBeVisible();
    await page.getByLabel("Dashboard Access Token *").fill(dashboardToken);
    await page.getByRole("button", { name: "Unlock Dashboard", exact: true }).click();
    await expect(login).toBeHidden();
    await expect(active).toBeVisible();

    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByLabel("Storage Driver *").selectOption("cloud");
    await page.getByLabel("Database Path / Endpoint URL *").fill(remoteUrl);
    await page.getByLabel("Remote Database Token (optional)").fill(remoteToken);
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.locator("header").getByText(remoteUrl, { exact: true })).toBeVisible();
    await page.reload();
    await expect(active).toBeVisible();
    await expect(login).toBeHidden();
    await expect(page.locator("header").getByText(remoteUrl, { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Collections & Objects", exact: true }).click();
    await expect(page.getByText("smoke-remote", { exact: true })).toBeVisible();

    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByLabel("Remote Database Token (optional)")).toHaveValue("");
    await page.getByLabel("Storage Driver *").selectOption("memory");
    await page.getByLabel("Database Path / Endpoint URL *").fill(":memory:");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect(page.locator("header").getByText(":memory:", { exact: true })).toBeVisible();
    await page.reload();
    await expect(active).toBeVisible();
    await expect(login).toBeHidden();
    await expect(page.locator("header").getByText(":memory:", { exact: true })).toBeVisible();
    expect(remoteAuth.length).toBeGreaterThan(0);
    expect(remoteAuth.every((header) => header === `Bearer ${remoteToken}`)).toBe(true);
    expect(pageErrors).toEqual([]);
  } finally {
    await page.close();
    await dashboard?.close();
    upstream.closeAllConnections();
    if (upstream.listening) {
      await new Promise((resolve, reject) => {
        upstream.close((error) => {
          if (error) {
            reject(error);
          } else {
            resolve();
          }
        });
      });
    }
    await remoteDb.close();
  }
});
