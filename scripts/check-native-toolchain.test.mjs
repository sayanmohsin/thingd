import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

test("strict diagnostics resolve LLVM symlinks and detect separate installations", (t) => {
  const root = mkdtempSync(join(tmpdir(), "thingd-llvm-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, "bin");
  const prefix = join(root, "cellar", "llvm");
  const lib = join(prefix, "lib");
  const other = join(root, "other");
  mkdirSync(bin, { recursive: true });
  mkdirSync(lib, { recursive: true });
  mkdirSync(other);
  symlinkSync(prefix, join(root, "opt-llvm"), "dir");
  for (const [name, output] of [["rustc", "rustc 1.0"], ["clang", "clang 1.0"]]) {
    writeFileSync(join(bin, name), `#!/bin/sh\nprintf '%s\\n' '${output}'\n`, { mode: 0o755 });
  }
  writeFileSync(join(bin, "llvm-config"), `#!/bin/sh\nif [ "$1" = "--prefix" ]; then printf '%s\\n' '${prefix}'; else printf '1.0\\n'; fi\n`, { mode: 0o755 });
  const run = (path) => spawnSync(process.execPath, ["scripts/check-native-toolchain.mjs", "--json", "--strict"], {
    encoding: "utf8",
    env: { ...process.env, PATH: bin, LIBCLANG_PATH: path, LLVM_CONFIG_PATH: join(bin, "llvm-config"), DYLD_LIBRARY_PATH: "" },
  });
  const linked = run(join(root, "opt-llvm", "lib"));
  assert.equal(linked.status, 0, linked.stdout + linked.stderr);
  const mismatch = run(other);
  assert.equal(mismatch.status, 1);
  assert.match(mismatch.stdout, /different LLVM installations/);
});
