import { defineConfig } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";

// Resolves the real, already-built `omni` CLI binary. Prefers a release
// build if one exists (closer to what a real user runs), falls back to
// debug (what `cargo build -p omni-cli` produces by default). Throws a
// clear error rather than silently falling back to `cargo run` (which
// would mask build failures behind Playwright's own webServer timeout).
function resolveOmniBinary(): string {
  const exeName = process.platform === "win32" ? "omni.exe" : "omni";
  const repoRoot = path.resolve(__dirname, "..", "..");
  const candidates = [
    path.join(repoRoot, "target", "release", exeName),
    path.join(repoRoot, "target", "debug", exeName),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  throw new Error(
    `could not find a built 'omni' binary in target/release or target/debug. ` +
      `Run 'cargo build -p omni-cli' (or '--release') from the repo root first.`,
  );
}

const exampleDir = path.resolve(__dirname, "..", "..", "examples", "hello-fullstack");

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  fullyParallel: false,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:4280",
    trace: "retain-on-failure",
  },
  // Starts the real, unified `omni dev` (both `web` and `backend` targets,
  // using the fixed ports examples/hello-fullstack/omni.yaml declares --
  // 4280/4281 -- which is also what web/app.yaml's baked-in `api_base`
  // expects) and waits for the web target to actually respond before
  // running any test. Teardown (on test completion) terminates the
  // process tree; see README.md for why that's a hard stop rather than
  // this suite also exercising the graceful-shutdown path (that's covered
  // by `crates/omni-cli/tests/dev_lifecycle.rs`, in Rust, where the exact
  // OS signal delivered can actually be controlled and asserted on).
  webServer: {
    command: `"${resolveOmniBinary()}" dev`,
    cwd: exampleDir,
    url: "http://127.0.0.1:4280",
    timeout: 30_000,
    reuseExistingServer: false,
  },
});
