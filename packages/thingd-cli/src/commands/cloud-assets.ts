import { createHash } from "node:crypto";
import { existsSync, lstatSync, readFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { type CliContext, requiredFlag, requiredToken, writeJson } from "../index.js";
import {
  beginAppAssetUpload,
  type CloudAppAsset,
  type CloudProject,
  listAppAssets,
  listProjects,
  uploadAppAsset,
} from "../lib/cloud-api.js";
import { readCloudConfig } from "../lib/cloud-config.js";

const MIME_BY_EXTENSION: Record<string, string> = {
  ".webp": "image/webp",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
};

function findProject(projects: CloudProject[], value: string): CloudProject {
  const project = projects.find((candidate) => candidate.id === value || candidate.slug === value);
  if (!project) {
    throw new Error(`Project not found: ${value}`);
  }
  return project;
}

async function resolveProject(value: string): Promise<CloudProject> {
  const config = readCloudConfig();
  if (!config?.userToken && !config?.token) {
    throw new Error("Not logged in. Run `thingd cloud login` first.");
  }
  const { projects } = await listProjects({
    ...config,
    url: config.url ?? "https://api.thingd.cloud",
  });
  return findProject(projects, value);
}

export async function runCloudAssets(context: CliContext): Promise<void> {
  const action = requiredToken(context.parsed, 2, "asset command");
  const config = readCloudConfig();
  if (!config) {
    throw new Error("Not logged in. Run `thingd cloud login` first.");
  }

  if (action === "list") {
    const project = await resolveProject(requiredFlag(context.parsed, "project"));
    const assets: CloudAppAsset[] = [];
    let cursor: string | undefined;
    do {
      const result = await listAppAssets(config, project.id, cursor);
      assets.push(...result.assets);
      cursor = result.nextCursor ?? undefined;
    } while (cursor);
    writeJson(context.stdout, { assets }, context.pretty);
    return;
  }

  if (action !== "upload") {
    throw new Error("Usage: thingd cloud assets <list|upload> --project <project> [--file <path>]");
  }

  const filePath = resolve(requiredFlag(context.parsed, "file"));
  if (!existsSync(filePath)) {
    throw new Error(`Asset file not found: ${filePath}`);
  }
  if (!lstatSync(filePath).isFile()) {
    throw new Error("Asset path must refer to a regular file, not a symlink or directory");
  }
  const extension = filePath.slice(filePath.lastIndexOf(".")).toLowerCase();
  const contentType = MIME_BY_EXTENSION[extension];
  if (!contentType) {
    throw new Error("Supported asset files: .webp, .png, .jpg, .jpeg");
  }
  if (lstatSync(filePath).size > 2 * 1024 * 1024) {
    throw new Error("Asset exceeds the current 2 MiB Cloud upload limit");
  }
  const bytes = readFileSync(filePath);
  if (bytes.byteLength > 2 * 1024 * 1024) {
    throw new Error("Asset exceeds the current 2 MiB Cloud upload limit");
  }
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const signatureValid =
    contentType === "image/png"
      ? bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))
      : contentType === "image/jpeg"
        ? bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff
        : bytes.subarray(0, 4).toString("ascii") === "RIFF" &&
          bytes.subarray(8, 12).toString("ascii") === "WEBP";
  if (!signatureValid) {
    throw new Error("File extension does not match a supported image signature");
  }
  const project = await resolveProject(requiredFlag(context.parsed, "project"));
  const upload = await beginAppAssetUpload(config, project.id, {
    fileName: basename(filePath),
    contentType,
    sizeBytes: bytes.byteLength,
    idempotencyKey: sha256,
  });

  if (!("uploadId" in upload)) {
    writeJson(context.stdout, { asset: upload.asset, reused: true }, context.pretty);
    return;
  }
  const result = await uploadAppAsset(config, project.id, upload.uploadId, contentType, bytes);
  writeJson(context.stdout, { asset: result.asset, reused: result.reused }, context.pretty);
}
