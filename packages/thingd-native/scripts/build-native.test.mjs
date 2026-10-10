import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { nativeBuildEnvironment } from "./build-native.mjs";

describe("native macOS build environment", () => {
  it("uses the aligned Mach-O layout on macOS 26 and later by default", () => {
    const environment = { PATH: "/usr/bin" };

    assert.deepEqual(
      nativeBuildEnvironment({ platform: "darwin", version: "27.0.1", environment }),
      { PATH: "/usr/bin", MACOSX_DEPLOYMENT_TARGET: "11.0" }
    );
  });

  it("leaves older macOS builds unchanged", () => {
    const environment = { PATH: "/usr/bin" };

    assert.equal(
      nativeBuildEnvironment({ platform: "darwin", version: "25.7.0", environment }),
      environment
    );
  });

  it("does not apply the Apple linker workaround on other platforms", () => {
    const environment = { PATH: "/usr/bin" };

    assert.equal(
      nativeBuildEnvironment({ platform: "linux", version: "27.0.1", environment }),
      environment
    );
  });

  it("preserves an explicitly selected deployment target", () => {
    const environment = { MACOSX_DEPLOYMENT_TARGET: "13.0" };

    assert.equal(
      nativeBuildEnvironment({ platform: "darwin", version: "27.0.1", environment }),
      environment
    );
  });

  it("does not guess when the host version is unavailable", () => {
    const environment = { PATH: "/usr/bin" };

    assert.equal(
      nativeBuildEnvironment({ platform: "darwin", version: "", environment }),
      environment
    );
  });
});
