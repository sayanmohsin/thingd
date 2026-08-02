import { NativeThingStore } from "@thingd/sdk";
import pc from "picocolors";
import { type CliContext, resolveConnection } from "./index.js";

export async function runDoctor(context: CliContext): Promise<void> {
  context.stderr.write(`\n${pc.bold("thingd doctor")}\n`);
  context.stderr.write(`${pc.dim("Running system diagnostics and connectivity tests...")}\n\n`);

  let healthy = true;

  // 1. Check Node version
  const nodeVersion = process.version;
  const majorVersion = Number.parseInt(nodeVersion.slice(1).split(".")[0] ?? "0", 10);
  if (majorVersion >= 20) {
    context.stderr.write(`  ${pc.green("✓")} Node version:   ${pc.cyan(nodeVersion)} (OK)\n`);
  } else {
    healthy = false;
    context.stderr.write(
      `  ${pc.red("×")} Node version:   ${pc.yellow(nodeVersion)} (Requires >= v20.x)\n`
    );
  }

  // 2. Resolve connection options
  const connection = resolveConnection(context);

  // 3. Native Driver Checks
  if (connection.driver === "native") {
    try {
      const hasNative = await NativeThingStore.isAvailable();
      if (hasNative) {
        const loadedPath = (await NativeThingStore.getLoadedPath()) ?? "unknown";
        const buildType = loadedPath.includes("prebuilds")
          ? pc.cyan("Loaded prebuilt driver")
          : pc.cyan("Loaded local development build");

        context.stderr.write(
          `  ${pc.green("✓")} Native Addon:    ${buildType} (${pc.dim(loadedPath)})\n`
        );
      } else {
        healthy = false;
        context.stderr.write(
          `  ${pc.red("×")} Native Addon:    ${pc.yellow('Not found. Run "pnpm --filter thingd-native build" or configure THINGD_NATIVE_PATH.')}\n`
        );
      }
    } catch (error) {
      healthy = false;
      context.stderr.write(
        `  ${pc.red("×")} Native Addon:    ${pc.yellow(`Failed to load native addon: ${error instanceof Error ? error.message : String(error)}`)}\n`
      );
    }
  } else {
    context.stderr.write(
      `  ${pc.dim("○")} Native Addon:    Skipped (Using driver: "${connection.driver ?? "memory"}")\n`
    );
  }

  // 4. Remote Sidecar Reachability & Auth Checks
  if (connection.cloud) {
    const rawUrl = connection.path;
    const isLocal = rawUrl.includes("localhost") || rawUrl.includes("127.0.0.1");

    if (!connection.authToken && !isLocal) {
      context.stderr.write(
        `  ${pc.yellow("⚠")} Auth Token:     ${pc.yellow("Missing THINGD_AUTH_TOKEN for remote server (might fail)")}\n`
      );
    } else if (connection.authToken) {
      context.stderr.write(`  ${pc.green("✓")} Auth Token:     ${pc.cyan("Configured")}\n`);
    } else {
      context.stderr.write(
        `  ${pc.green("✓")} Auth Token:     ${pc.dim("Not required for local sidecar")}\n`
      );
    }

    try {
      const targetUrl = new URL(
        rawUrl.startsWith("thingd://") ? `http://${rawUrl.slice("thingd://".length)}` : rawUrl
      );
      if (targetUrl.pathname === "/mcp" || targetUrl.pathname === "") {
        targetUrl.pathname = "/healthz";
      } else {
        targetUrl.pathname = `${targetUrl.pathname.replace(/\/mcp$/, "")}/healthz`;
      }

      context.stderr.write(
        `  ${pc.dim("○")} Connectivity:   Checking reachability to ${pc.cyan(targetUrl.toString())}...\n`
      );

      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 3000);

      const headers: Record<string, string> = {};
      if (connection.authToken) {
        headers.Authorization = `Bearer ${connection.authToken}`;
      }

      try {
        const response = await fetch(targetUrl, {
          signal: controller.signal,
          headers,
        });
        clearTimeout(timeoutId);

        if (response.ok) {
          context.stderr.write(
            `  ${pc.green("✓")} Connectivity:   ${pc.cyan("Connected successfully!")} (${pc.dim(`HTTP ${response.status}`)})\n`
          );
        } else {
          healthy = false;
          context.stderr.write(
            `  ${pc.red("×")} Connectivity:   ${pc.yellow(`Server responded with non-2xx status`)} (${pc.dim(`HTTP ${response.status}`)})\n`
          );
        }
      } catch (fetchError) {
        clearTimeout(timeoutId);
        healthy = false;
        context.stderr.write(
          `  ${pc.red("×")} Connectivity:   ${pc.yellow(`Failed to connect. Connection refused or timed out.`)} (${pc.dim(fetchError instanceof Error ? fetchError.message : String(fetchError))})\n`
        );
      }
    } catch (urlError) {
      healthy = false;
      context.stderr.write(
        `  ${pc.red("×")} Connectivity:   ${pc.yellow(`Invalid URL structure: ${urlError instanceof Error ? urlError.message : String(urlError)}`)}\n`
      );
    }
  } else {
    context.stderr.write(
      `  ${pc.green("✓")} Connectivity:   ${pc.cyan("Local persistent store")} (${pc.dim(connection.path)})\n`
    );
  }

  // Final Summary Report
  context.stderr.write("\n");
  if (healthy) {
    context.stderr.write(`  ${pc.bold(pc.green("Diagnosis: Everything looks healthy!"))}\n\n`);
  } else {
    context.stderr.write(
      `  ${pc.bold(pc.yellow("Diagnosis: Some items require attention (see errors above)."))}\n\n`
    );
  }
}
