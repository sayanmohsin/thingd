import { execFile, execFileSync } from "node:child_process";
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const METADATA_NAME = "thingd-backup.json";

type ArchiveMetadata = {
  format: "thingd-native-tar";
  version: 1;
  sourceName: string;
  createdAt: string;
};

async function runTar(args: string[]): Promise<void> {
  try {
    await execFileAsync("tar", args, { maxBuffer: 16 * 1024 * 1024 });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`tar operation failed: ${detail}`);
  }
}

function assertDirectory(path: string, label: string): void {
  if (!existsSync(path) || !lstatSync(path).isDirectory()) {
    throw new Error(`${label} must be an existing directory: ${path}`);
  }
}

function assertArchivePathSafe(path: string): void {
  if (path.startsWith("/") || path.startsWith("\\") || /^[A-Za-z]:[\\/]/.test(path)) {
    throw new Error(`Unsafe archive path: ${path}`);
  }
  const normalized = path.replaceAll("\\", "/");
  if (normalized.split("/").includes("..")) {
    throw new Error(`Unsafe archive path: ${path}`);
  }
}

function archiveEntries(archive: string): string[] {
  const result = execFileSync("tar", ["-tf", archive], {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  }) as string;
  return result.split(/\r?\n/).filter(Boolean);
}

export async function createNativeArchive(sourcePath: string, outputPath: string): Promise<void> {
  const source = resolve(sourcePath);
  const output = resolve(outputPath);
  assertDirectory(source, "Native database");
  const outputRelative = relative(source, output);
  if (outputRelative && !outputRelative.startsWith("..") && !isAbsolute(outputRelative)) {
    throw new Error("Backup archive must be outside the source database directory");
  }
  if (existsSync(output)) {
    throw new Error(`Backup destination already exists: ${output}`);
  }
  mkdirSync(dirname(output), { recursive: true });

  const staging = mkdtempSync(join(tmpdir(), "thingd-backup-"));
  const metadata: ArchiveMetadata = {
    format: "thingd-native-tar",
    version: 1,
    sourceName: basename(source),
    createdAt: new Date().toISOString(),
  };
  const metadataPath = join(staging, METADATA_NAME);
  writeFileSync(metadataPath, `${JSON.stringify(metadata, null, 2)}\n`, "utf8");
  try {
    await runTar([
      "-cf",
      output,
      "-C",
      dirname(source),
      basename(source),
      "-C",
      staging,
      METADATA_NAME,
    ]);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

export async function restoreNativeArchive(
  archivePath: string,
  destinationPath: string,
  replace: boolean
): Promise<void> {
  const archive = resolve(archivePath);
  const destination = resolve(destinationPath);
  if (!existsSync(archive) || !lstatSync(archive).isFile()) {
    throw new Error(`Backup archive not found: ${archive}`);
  }
  if (existsSync(destination) && !replace) {
    throw new Error(
      `Restore destination already exists: ${destination}; pass --replace to replace it`
    );
  }
  mkdirSync(dirname(destination), { recursive: true });

  const entries = archiveEntries(archive);
  for (const entry of entries) {
    assertArchivePathSafe(entry);
  }
  if (!entries.includes(METADATA_NAME)) {
    throw new Error(`Invalid Thingd archive: missing ${METADATA_NAME}`);
  }
  const roots = new Set(
    entries
      .filter((entry) => entry !== METADATA_NAME)
      .map((entry) => entry.split("/")[0])
      .filter(Boolean)
  );
  if (roots.size !== 1) {
    throw new Error("Invalid Thingd archive: expected exactly one database directory");
  }

  const stagingParent = mkdtempSync(join(dirname(destination), ".thingd-restore-"));
  const root = [...roots][0];
  if (!root) {
    throw new Error("Invalid Thingd archive: missing database directory");
  }
  const staging = join(stagingParent, root);
  let previous: string | undefined;
  try {
    await runTar(["-xf", archive, "-C", stagingParent]);
    const metadata = JSON.parse(
      readFileSync(join(stagingParent, METADATA_NAME), "utf8")
    ) as ArchiveMetadata;
    if (metadata.format !== "thingd-native-tar" || metadata.version !== 1) {
      throw new Error("Invalid Thingd archive metadata");
    }
    assertDirectory(staging, "Restored native database");
    if (!existsSync(join(staging, "lock")) || !existsSync(join(staging, "keyspaces"))) {
      throw new Error("Invalid Thingd archive: missing lock or keyspaces directory");
    }

    if (replace && existsSync(destination)) {
      previous = `${destination}.previous-${Date.now()}`;
      renameSync(destination, previous);
    }
    renameSync(staging, destination);
    if (previous) {
      rmSync(previous, { recursive: true, force: true });
    }
  } catch (error) {
    if (previous && !existsSync(destination) && existsSync(previous)) {
      renameSync(previous, destination);
    }
    throw error;
  } finally {
    rmSync(stagingParent, { recursive: true, force: true });
  }
}
