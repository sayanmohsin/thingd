import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
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
      `  ${pc.red("×")} Node version:   ${pc.yellow(nodeVersion)} (Requires >= v20.x)\n`,
    );
  }

  // 2. Resolve connection options
  const connection = resolveConnection(context);

  // 3. Native Driver Checks
  if (connection.driver === "native") {
    const customPath = process.env.THINGD_NATIVE_PATH;
    if (customPath) {
      if (existsSync(customPath)) {
        try {
          const require = createRequire(import.meta.url);
          const binding = require(customPath);
          if (binding?.NativeThingStore) {
            context.stderr.write(
              `  ${pc.green("✓")} Native Binding: ${pc.cyan("Loaded via THINGD_NATIVE_PATH")} (${pc.dim(customPath)})\n`,
            );
          } else {
            healthy = false;
            context.stderr.write(
              `  ${pc.red("×")} Native Binding: ${pc.yellow("Loaded but missing NativeThingStore export")} (${pc.dim(customPath)})\n`,
            );
          }
        } catch (error) {
          healthy = false;
          context.stderr.write(
            `  ${pc.red("×")} Native Binding: ${pc.yellow(`Failed to load: ${error instanceof Error ? error.message : String(error)}`)} (${pc.dim(customPath)})\n`,
          );
        }
      } else {
        healthy = false;
        context.stderr.write(
          `  ${pc.red("×")} Native Binding: ${pc.yellow("File does not exist")} at THINGD_NATIVE_PATH="${pc.dim(customPath)}"\n`,
        );
      }
    } else {
      // Auto-detect sibling binary
      let detectedPath: string | null = null;
      try {
        const scriptPath = process.argv[1];
        if (scriptPath) {
          const cliDir = join(resolve(scriptPath), "..", "..");
          const candidates = [
            join(cliDir, "node_modules", "thingd-native", "dist", "thingd_native.node"),
            join(cliDir, "..", "thingd-native", "dist", "thingd_native.node"),
            join(
              homedir(),
              "Space/Programming/personal/thingd/packages/thingd-native/dist/thingd_native.node",
            ),
            join(
              homedir(),
              "Space/Programming/personal/thingd-cloud/packages/thingd-native/dist/thingd_native.node",
            ),
          ];
          for (const candidate of candidates) {
            if (existsSync(candidate)) {
              detectedPath = candidate;
              break;
            }
          }
        }
      } catch {
        // Ignore
      }

      if (detectedPath) {
        try {
          const require = createRequire(import.meta.url);
          const binding = require(detectedPath);
          if (binding?.NativeThingStore) {
            context.stderr.write(
              `  ${pc.green("✓")} Native Binding: ${pc.cyan("Auto-detected and loaded successfully")} (${pc.dim(detectedPath)})\n`,
            );
          } else {
            healthy = false;
            context.stderr.write(
              `  ${pc.red("×")} Native Binding: ${pc.yellow("Auto-detected but missing NativeThingStore export")} (${pc.dim(detectedPath)})\n`,
            );
          }
        } catch (error) {
          healthy = false;
          context.stderr.write(
            `  ${pc.red("×")} Native Binding: ${pc.yellow(`Failed to load auto-detected binding: ${error instanceof Error ? error.message : String(error)}`)} (${pc.dim(detectedPath)})\n`,
          );
        }
      } else {
        healthy = false;
        context.stderr.write(
          `  ${pc.red("×")} Native Binding: ${pc.yellow('Not found. Run "pnpm --filter thingd-native build" or configure THINGD_NATIVE_PATH.')}\n`,
        );
      }
    }
  } else {
    context.stderr.write(
      `  ${pc.dim("○")} Native Binding: Skipped (Using driver: "${connection.driver ?? "memory"}")\n`,
    );
  }

  // 4. Remote Sidecar Reachability & Auth Checks
  if (connection.cloud) {
    const rawUrl = connection.path;
    const isLocal = rawUrl.includes("localhost") || rawUrl.includes("127.0.0.1");

    if (!connection.authToken && !isLocal) {
      context.stderr.write(
        `  ${pc.yellow("⚠")} Auth Token:     ${pc.yellow("Missing THINGD_AUTH_TOKEN for remote server (might fail)")}\n`,
      );
    } else if (connection.authToken) {
      context.stderr.write(`  ${pc.green("✓")} Auth Token:     ${pc.cyan("Configured")}\n`);
    } else {
      context.stderr.write(
        `  ${pc.green("✓")} Auth Token:     ${pc.dim("Not required for local sidecar")}\n`,
      );
    }

    try {
      const normalizeUrl = (val: string) =>
        val.startsWith("thingd://") ? `http://${val.slice("thingd://".length)}` : val;
      const targetUrl = new URL(normalizeUrl(rawUrl));
      if (targetUrl.pathname === "/mcp" || targetUrl.pathname === "") {
        targetUrl.pathname = "/healthz";
      } else {
        targetUrl.pathname = `${targetUrl.pathname.replace(/\/mcp$/, "")}/healthz`;
      }

      context.stderr.write(
        `  ${pc.dim("○")} Connectivity:   Checking reachability to ${pc.cyan(targetUrl.toString())}...\n`,
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
            `  ${pc.green("✓")} Connectivity:   ${pc.cyan("Connected successfully!")} (${pc.dim(`HTTP ${response.status}`)})\n`,
          );
        } else {
          healthy = false;
          context.stderr.write(
            `  ${pc.red("×")} Connectivity:   ${pc.yellow(`Server responded with non-2xx status`)} (${pc.dim(`HTTP ${response.status}`)})\n`,
          );
        }
      } catch (fetchError) {
        clearTimeout(timeoutId);
        healthy = false;
        context.stderr.write(
          `  ${pc.red("×")} Connectivity:   ${pc.yellow(`Failed to connect. Connection refused or timed out.`)} (${pc.dim(fetchError instanceof Error ? fetchError.message : String(fetchError))})\n`,
        );
      }
    } catch (urlError) {
      healthy = false;
      context.stderr.write(
        `  ${pc.red("×")} Connectivity:   ${pc.yellow(`Invalid URL structure: ${urlError instanceof Error ? urlError.message : String(urlError)}`)}\n`,
      );
    }
  } else {
    context.stderr.write(
      `  ${pc.green("✓")} Connectivity:   ${pc.cyan("Local SQLite Store")} (${pc.dim(connection.path)})\n`,
    );
  }

  // Final Summary Report
  context.stderr.write("\n");
  if (healthy) {
    context.stderr.write(`  ${pc.bold(pc.green("Diagnosis: Everything looks healthy!"))}\n\n`);
  } else {
    context.stderr.write(
      `  ${pc.bold(pc.yellow("Diagnosis: Some items require attention (see errors above)."))}\n\n`,
    );
  }
}
