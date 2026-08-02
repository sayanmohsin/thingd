import fs from "node:fs/promises";
import semanticRelease from "semantic-release";

const retryVersion = process.env.RETRY_VERSION;
const outputFile = process.env.GITHUB_OUTPUT;

let result;
if (retryVersion) {
  if (!/^\d+\.\d+\.\d+$/.test(retryVersion)) {
    throw new Error(`RETRY_VERSION must be a full SemVer value, received: ${retryVersion}`);
  }
  result = { needed: true, retry: true, version: retryVersion, ref: `v${retryVersion}` };
} else {
  const release = await semanticRelease({ dryRun: true, ci: true }, { cwd: process.cwd() });
  const nextRelease = release?.nextRelease;
  result = {
    needed: Boolean(nextRelease),
    retry: false,
    version: nextRelease?.version ?? "",
    ref: "main",
    releaseType: nextRelease?.type ?? "",
  };
}

if (outputFile) {
  await fs.appendFile(
    outputFile,
    `needed=${result.needed}\nretry=${result.retry}\nversion=${result.version}\nref=${result.ref}\n`,
  );
}

process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
