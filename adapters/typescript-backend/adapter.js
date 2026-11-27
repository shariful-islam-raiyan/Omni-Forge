#!/usr/bin/env node
/**
 * OmniForge typescript-backend adapter.
 *
 * Structurally identical to node-backend's adapter.js (same wire protocol,
 * same path-safety checks, same input merging, same runnable-package
 * artifact layout) with one real difference: the component's source is a
 * `.ts` file, genuinely type-checked and compiled by the real TypeScript
 * compiler (`tsc`) before being run with `node` -- not transpiled-only,
 * not faked. See docs/decisions/0013-typescript-backend-adapter.md.
 *
 * `validate` runs `tsc --noEmit` (type-check only, writes nothing, as the
 * Adapter trait's contract requires). `compile` runs `tsc` for real,
 * capturing genuine compiler diagnostics, then executes the compiled
 * JavaScript with `node` exactly like node-backend does.
 *
 * Uses only Node's built-in modules for its own logic; the TypeScript
 * compiler is resolved at runtime (see `resolveTsc`), never bundled.
 */
"use strict";

const fs = require("fs");
const path = require("path");
const os = require("os");
const cp = require("child_process");

const PROTOCOL_VERSION = "omniforge.dev/adapter-protocol/v1";
const BACKEND_APP_SCHEMA = "omniforge.dev/backend-app/v1";
const ADAPTER_ID = "typescript-backend";
const ADAPTER_VERSION = "0.1.0";

// Minimal ambient declarations for the handful of Node.js globals this
// adapter's own example (and most small backend scripts) use, so `tsc
// --strict` doesn't fail merely because @types/node isn't installed --
// genuine type-checking of the *component's own* code still happens in
// full; this only stands in for Node's own ambient type declarations.
// Not a substitute for @types/node in a real project -- a real project
// should install it; this keeps the adapter dependency-free.
const NODE_GLOBALS_SHIM = `
declare const process: { argv: string[] };
declare function require(id: string): any;
`;

function log(message) {
  process.stderr.write(String(message) + "\n");
}

function diagnostic(code, message, severity, extra) {
  const entry = { code, severity: severity || "error", message };
  if (extra) {
    for (const key of Object.keys(extra)) {
      if (extra[key] !== null && extra[key] !== undefined) {
        entry[key] = extra[key];
      }
    }
  }
  return entry;
}

function respond(requestId, status, diagnostics, artifacts) {
  const payload = {
    protocol: PROTOCOL_VERSION,
    request_id: requestId,
    status,
    diagnostics: diagnostics || [],
    artifacts: artifacts || [],
  };
  process.stdout.write(JSON.stringify(payload) + "\n");
}

function stripVerbatimPrefix(p) {
  if (process.platform !== "win32" || typeof p !== "string") {
    return p;
  }
  if (p.startsWith("\\\\?\\UNC\\")) {
    return "\\\\" + p.slice("\\\\?\\UNC\\".length);
  }
  if (p.startsWith("\\\\?\\")) {
    return p.slice("\\\\?\\".length);
  }
  return p;
}

function isWithin(p, root) {
  let pr;
  let rr;
  try {
    pr = fs.realpathSync.native(p);
    rr = fs.realpathSync.native(root);
  } catch {
    return false;
  }
  const rel = path.relative(rr, pr);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

function hasError(diagnostics) {
  return diagnostics.some((d) => d.severity === "error");
}

function validateComponents(components, projectRoot) {
  const diagnostics = [];
  const safe = [];
  for (const component of components) {
    const name = component.name || "<unknown>";
    const sourcePath = component.source_path;
    if (!sourcePath) {
      diagnostics.push(
        diagnostic("E-TSADAPTER-MISSING-SOURCE", `component '${name}' has no resolved source path`, "error", { component: name })
      );
      continue;
    }
    if (!isWithin(sourcePath, projectRoot)) {
      diagnostics.push(
        diagnostic(
          "E-TSADAPTER-UNSAFE-PATH",
          `component '${name}' source path '${sourcePath}' resolves outside the project root`,
          "error",
          { component: name }
        )
      );
      continue;
    }
    let isFile = false;
    try {
      isFile = fs.statSync(sourcePath).isFile();
    } catch {
      isFile = false;
    }
    if (!isFile) {
      diagnostics.push(
        diagnostic("E-TSADAPTER-SOURCE-MISSING", `component '${name}' source file '${sourcePath}' does not exist`, "error", {
          component: name,
          file: sourcePath,
        })
      );
      continue;
    }
    if (!sourcePath.toLowerCase().endsWith(".ts")) {
      diagnostics.push(
        diagnostic(
          "E-TSADAPTER-NOT-TYPESCRIPT-SOURCE",
          `component '${name}' source '${sourcePath}' is not a .ts file`,
          "error",
          { component: name, file: sourcePath }
        )
      );
      continue;
    }
    safe.push(component);
  }
  return { safe, diagnostics };
}

function readEntry(entryPath, projectRoot) {
  if (!entryPath) {
    return { service: null, inputs: {}, diagnostics: [] };
  }
  if (!isWithin(entryPath, projectRoot)) {
    return {
      service: null,
      inputs: {},
      diagnostics: [diagnostic("E-TSADAPTER-UNSAFE-PATH", `entry file '${entryPath}' resolves outside the project root`, "error", { file: entryPath })],
    };
  }
  let isFile = false;
  try {
    isFile = fs.statSync(entryPath).isFile();
  } catch {
    isFile = false;
  }
  if (!isFile) {
    return {
      service: null,
      inputs: {},
      diagnostics: [diagnostic("E-TSADAPTER-ENTRY-MISSING", `entry file '${entryPath}' does not exist`, "error", { file: entryPath })],
    };
  }
  let doc;
  try {
    doc = JSON.parse(fs.readFileSync(entryPath, "utf8"));
  } catch (e) {
    return {
      service: null,
      inputs: {},
      diagnostics: [diagnostic("E-TSADAPTER-ENTRY-INVALID", `could not read/parse entry file '${entryPath}': ${e.message}`, "error", { file: entryPath })],
    };
  }
  if (doc.schema !== BACKEND_APP_SCHEMA) {
    return {
      service: null,
      inputs: {},
      diagnostics: [
        diagnostic(
          "E-TSADAPTER-ENTRY-SCHEMA-VERSION",
          `unsupported backend app manifest schema '${doc.schema}', expected '${BACKEND_APP_SCHEMA}'`,
          "error",
          { file: entryPath }
        ),
      ],
    };
  }
  return { service: doc.service || null, inputs: doc.inputs || {}, diagnostics: [] };
}

function mergeInputs(iface, entryInputs, componentName) {
  const declared = (iface && iface.inputs) || {};
  const merged = {};
  const diagnostics = [];

  for (const inputName of Object.keys(declared)) {
    const spec = declared[inputName];
    if (spec.default !== null && spec.default !== undefined) {
      merged[inputName] = spec.default;
    }
  }
  for (const inputName of Object.keys(entryInputs)) {
    if (!(inputName in declared)) {
      diagnostics.push(
        diagnostic("W-TSADAPTER-UNKNOWN-INPUT", `input '${inputName}' is not declared on component '${componentName}'`, "warning", {
          component: componentName,
        })
      );
      continue;
    }
    merged[inputName] = entryInputs[inputName];
  }
  for (const inputName of Object.keys(declared)) {
    const spec = declared[inputName];
    if (spec.required && !(inputName in merged)) {
      diagnostics.push(
        diagnostic(
          "E-TSADAPTER-MISSING-INPUT",
          `required input '${inputName}' for component '${componentName}' was not provided and has no default`,
          "error",
          { component: componentName }
        )
      );
    }
  }
  return { merged, diagnostics };
}

/** Finds a usable TypeScript compiler invocation. Tries a direct `tsc` on
 * PATH first (works if a user ran `npm install -g typescript`, or has it
 * as a project dependency with `.bin` on PATH); falls back to
 * `npx --yes --package typescript -- tsc`, which works with nothing more
 * than Node + npm installed (verified: this is NOT the same as plain
 * `npx tsc`, which resolves to an unrelated, differently-named npm
 * package -- `--package typescript -- tsc` names both the package and the
 * binary explicitly, unambiguously). Returns {cmd, baseArgs} or null. */
function resolveTsc() {
  // Off Windows, a direct `tsc` (if globally installed) or a bare `npx`
  // both spawn cleanly without a shell -- no special-casing needed.
  if (process.platform !== "win32") {
    const direct = cp.spawnSync("tsc", ["--version"], { encoding: "utf8", timeout: 10000 });
    if (!direct.error && direct.status === 0) {
      return { cmd: "tsc", baseArgs: [] };
    }
    const viaNpx = cp.spawnSync("npx", ["--yes", "--package", "typescript", "--", "tsc", "--version"], {
      encoding: "utf8",
      timeout: 60000,
    });
    if (!viaNpx.error && viaNpx.status === 0) {
      return { cmd: "npx", baseArgs: ["--yes", "--package", "typescript", "--", "tsc"] };
    }
    return null;
  }

  // Windows: both a globally-installed `tsc` and the `npx` launcher are
  // `.cmd` shims there. Node's own `child_process` deliberately refuses to
  // spawn a `.cmd`/`.bat` file without `shell: true` (hardened behavior,
  // following a real command-injection advisory) -- and this project does
  // not use `shell: true` anywhere, by design (see docs/security-model.md).
  // The no-shell-safe equivalent: invoke npm's own bundled `npx-cli.js`
  // (a plain JavaScript file, shipped alongside `node.exe` in every
  // standard Windows Node.js install) directly with `node` -- exactly the
  // same mechanism this adapter script itself already is.
  const npxCliJs = path.join(path.dirname(process.execPath), "node_modules", "npm", "bin", "npx-cli.js");
  if (!fs.existsSync(npxCliJs)) {
    return null;
  }
  const probeArgs = [npxCliJs, "--yes", "--package", "typescript", "--", "tsc", "--version"];
  const probe = cp.spawnSync(process.execPath, probeArgs, { encoding: "utf8", timeout: 60000 });
  if (!probe.error && probe.status === 0) {
    return { cmd: process.execPath, baseArgs: [npxCliJs, "--yes", "--package", "typescript", "--", "tsc"] };
  }
  return null;
}

/** Runs the real compiler. `emit` false performs `validate`'s type-check
 * only (`--noEmit`, writes nothing); true performs `compile`'s real
 * compile into `outDir`. Returns {jsPath, diagnostics}; diagnostics
 * containing an error means compilation/type-checking failed -- real tsc
 * output (where type errors are printed) is captured as `explanation`. */
function runTsc(tsc, sourcePath, shimPath, outDir, emit, timeoutMs, componentName) {
  const args = [
    ...tsc.baseArgs,
    stripVerbatimPrefix(sourcePath),
    stripVerbatimPrefix(shimPath),
    "--target",
    "ES2020",
    "--module",
    "CommonJS",
    "--lib",
    "ES2020,DOM",
    "--strict",
    "--skipLibCheck",
  ];
  if (emit) {
    args.push("--outDir", stripVerbatimPrefix(outDir));
  } else {
    args.push("--noEmit");
  }

  let result;
  try {
    result = cp.spawnSync(tsc.cmd, args, { timeout: timeoutMs, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  } catch (e) {
    return { jsPath: null, diagnostics: [diagnostic("E-TSADAPTER-TYPECHECK-FAILED", `failed to start tsc: ${e.message}`, "error", { component: componentName })] };
  }
  if ((result.error && result.error.code === "ETIMEDOUT") || result.signal) {
    return {
      jsPath: null,
      diagnostics: [
        diagnostic("E-TSADAPTER-TYPECHECK-TIMEOUT", `tsc did not finish type-checking '${sourcePath}' in time`, "error", { component: componentName }),
      ],
    };
  }
  if (result.error) {
    return { jsPath: null, diagnostics: [diagnostic("E-TSADAPTER-TYPECHECK-FAILED", `failed to start tsc: ${result.error.message}`, "error", { component: componentName })] };
  }
  if (result.status !== 0) {
    const tail = ((result.stdout || "") + (result.stderr || "")).slice(-4000);
    return {
      jsPath: null,
      diagnostics: [
        diagnostic("E-TSADAPTER-TYPECHECK-FAILED", `tsc reported type/compile errors for '${sourcePath}'`, "error", {
          component: componentName,
          explanation: tail || null,
        }),
      ],
    };
  }
  if (!emit) {
    return { jsPath: null, diagnostics: [] };
  }
  const basename = path.basename(sourcePath, ".ts");
  const jsPath = path.join(outDir, `${basename}.js`);
  if (!fs.existsSync(jsPath)) {
    return {
      jsPath: null,
      diagnostics: [diagnostic("E-TSADAPTER-TYPECHECK-FAILED", `tsc exited successfully but did not produce the expected output '${jsPath}'`, "error", { component: componentName })],
    };
  }
  return { jsPath, diagnostics: [] };
}

function runBackendScript(jsPath, configPath, projectRoot, timeoutS, componentName) {
  const timeoutMs = Math.round(Math.max(1000, timeoutS * 1000));
  let result;
  try {
    result = cp.spawnSync(process.execPath, [stripVerbatimPrefix(jsPath), stripVerbatimPrefix(configPath)], {
      cwd: stripVerbatimPrefix(projectRoot),
      timeout: timeoutMs,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    });
  } catch (e) {
    return { stdoutText: null, diagnostics: [diagnostic("E-TSADAPTER-EXECUTION-FAILED", `failed to start compiled script '${jsPath}': ${e.message}`, "error", { component: componentName })] };
  }
  if ((result.error && result.error.code === "ETIMEDOUT") || result.signal) {
    return {
      stdoutText: null,
      diagnostics: [diagnostic("E-TSADAPTER-EXECUTION-TIMEOUT", `compiled script '${jsPath}' did not finish within ${timeoutS.toFixed(1)}s`, "error", { component: componentName })],
    };
  }
  if (result.error) {
    return { stdoutText: null, diagnostics: [diagnostic("E-TSADAPTER-EXECUTION-FAILED", `failed to start compiled script '${jsPath}': ${result.error.message}`, "error", { component: componentName })] };
  }
  if (result.status !== 0) {
    const tail = (result.stderr || "").slice(-2000);
    return {
      stdoutText: null,
      diagnostics: [
        diagnostic("E-TSADAPTER-EXECUTION-FAILED", `compiled script '${jsPath}' exited with status ${result.status}`, "error", {
          component: componentName,
          explanation: tail || null,
        }),
      ],
    };
  }
  return { stdoutText: result.stdout || "", diagnostics: [] };
}

function writeArtifacts(outputDir, sourcePath, compiledJsPath, mergedInputs, stdoutText, componentName) {
  const artifacts = [];
  const scriptBasename = path.basename(compiledJsPath);
  const appDir = path.join(outputDir, "app");
  fs.mkdirSync(appDir, { recursive: true });
  fs.copyFileSync(compiledJsPath, path.join(appDir, scriptBasename));
  artifacts.push(`app/${scriptBasename}`);

  fs.copyFileSync(sourcePath, path.join(outputDir, "source.ts"));
  artifacts.push("source.ts");

  fs.writeFileSync(path.join(outputDir, "run-config.json"), JSON.stringify(mergedInputs, Object.keys(mergedInputs).sort(), 2), "utf8");
  artifacts.push("run-config.json");

  fs.writeFileSync(path.join(outputDir, "output.txt"), stdoutText, "utf8");
  artifacts.push("output.txt");

  fs.writeFileSync(
    path.join(outputDir, "run.sh"),
    '#!/bin/sh\nDIR="$(cd "$(dirname "$0")" && pwd)"\n' + `exec node "$DIR/app/${scriptBasename}" "$DIR/run-config.json"\n`,
    { encoding: "utf8" }
  );
  artifacts.push("run.sh");

  fs.writeFileSync(path.join(outputDir, "run.bat"), `@echo off\r\nnode "%~dp0app\\${scriptBasename}" "%~dp0run-config.json"\r\n`, { encoding: "utf8" });
  artifacts.push("run.bat");

  const manifest = {
    schema: "omniforge.dev/backend-build-manifest/v1",
    adapter: { id: ADAPTER_ID, version: ADAPTER_VERSION },
    component: componentName,
    artifacts: [...artifacts, "build-manifest.json"].sort(),
  };
  fs.writeFileSync(path.join(outputDir, "build-manifest.json"), JSON.stringify(manifest, null, 2), "utf8");
  artifacts.push("build-manifest.json");

  return artifacts;
}

function writeShim() {
  const shimPath = path.join(__dirname, "node-globals.d.ts");
  fs.writeFileSync(shimPath, NODE_GLOBALS_SHIM, "utf8");
  return shimPath;
}

function handle(request) {
  const requestId = request.request_id || "unknown";

  if (request.protocol !== PROTOCOL_VERSION) {
    respond(requestId, "error", [diagnostic("E-TSADAPTER-PROTOCOL-VERSION", `unsupported protocol '${request.protocol}', expected '${PROTOCOL_VERSION}'`)]);
    return;
  }

  const operation = request.operation;
  if (operation !== "validate" && operation !== "compile") {
    respond(requestId, "error", [
      diagnostic("E-TSADAPTER-UNSUPPORTED-OPERATION", `operation '${operation}' is not supported by the ${ADAPTER_ID} adapter`, "error", {
        suggestion: "supported operations: validate, compile",
      }),
    ]);
    return;
  }

  const projectRoot = request.project_root || ".";
  const outputDir = request.output_dir || ".";
  const entryPath = request.entry_path;
  const components = (request.config && request.config.components) || [];

  if (components.length === 0) {
    respond(requestId, "error", [diagnostic("E-TSADAPTER-NO-COMPONENTS", "no components were provided to build for the 'backend' target")]);
    return;
  }

  const { safe: safeComponents, diagnostics } = validateComponents(components, projectRoot);
  if (safeComponents.length === 0) {
    respond(requestId, "error", diagnostics);
    return;
  }

  if (!isWithin(outputDir, projectRoot)) {
    diagnostics.push(diagnostic("E-TSADAPTER-UNSAFE-PATH", `output directory '${outputDir}' resolves outside the project root`));
    respond(requestId, "error", diagnostics);
    return;
  }

  const entryResult = readEntry(entryPath, projectRoot);
  diagnostics.push(...entryResult.diagnostics);
  if (hasError(entryResult.diagnostics)) {
    respond(requestId, "error", diagnostics);
    return;
  }

  let primary;
  if (entryResult.service) {
    primary = safeComponents.find((c) => c.name === entryResult.service);
    if (!primary) {
      diagnostics.push(diagnostic("E-TSADAPTER-SERVICE-NOT-FOUND", `entry file names service '${entryResult.service}', which was not resolved for this build`));
      respond(requestId, "error", diagnostics);
      return;
    }
  } else {
    primary = safeComponents[safeComponents.length - 1];
  }

  const mergeResult = mergeInputs(primary.interface, entryResult.inputs, primary.name);
  diagnostics.push(...mergeResult.diagnostics);
  if (hasError(mergeResult.diagnostics)) {
    respond(requestId, "error", diagnostics);
    return;
  }

  const tsc = resolveTsc();
  if (!tsc) {
    diagnostics.push(
      diagnostic("E-ADAPTER-TOOLCHAIN-MISSING", "no usable TypeScript compiler found (tried 'tsc' directly, then 'npx --package typescript -- tsc')", "error", {
        suggestion: "install TypeScript (npm install -g typescript), or ensure npm/npx can reach the registry",
      })
    );
    respond(requestId, "error", diagnostics);
    return;
  }
  const shimPath = writeShim();

  const totalBudgetS = Math.max(2.0, (request.timeout_ms || 60000) / 1000.0 - 2.0);
  const typecheckTimeoutMs = Math.round(Math.max(5000, totalBudgetS * 1000 * 0.8));
  const runTimeoutS = Math.max(1.0, totalBudgetS * 0.2);

  if (operation === "validate") {
    const tcResult = runTsc(tsc, primary.source_path, shimPath, null, false, typecheckTimeoutMs, primary.name);
    diagnostics.push(...tcResult.diagnostics);
    respond(requestId, hasError(tcResult.diagnostics) ? "error" : "ok", diagnostics);
    return;
  }

  try {
    fs.mkdirSync(outputDir, { recursive: true });
  } catch (e) {
    diagnostics.push(diagnostic("E-TSADAPTER-WRITE-FAILED", `could not create output directory: ${e.message}`));
    respond(requestId, "error", diagnostics);
    return;
  }

  const buildDir = fs.mkdtempSync(path.join(os.tmpdir(), "omniforge-tsc-"));
  const tcResult = runTsc(tsc, primary.source_path, shimPath, buildDir, true, typecheckTimeoutMs, primary.name);
  diagnostics.push(...tcResult.diagnostics);
  if (hasError(tcResult.diagnostics)) {
    fs.rmSync(buildDir, { recursive: true, force: true });
    respond(requestId, "error", diagnostics);
    return;
  }

  const configPath = path.join(outputDir, "run-config.json");
  try {
    fs.writeFileSync(configPath, JSON.stringify(mergeResult.merged, Object.keys(mergeResult.merged).sort(), 2), "utf8");
  } catch (e) {
    fs.rmSync(buildDir, { recursive: true, force: true });
    diagnostics.push(diagnostic("E-TSADAPTER-WRITE-FAILED", `could not write run-config.json: ${e.message}`));
    respond(requestId, "error", diagnostics);
    return;
  }

  const runResult = runBackendScript(tcResult.jsPath, configPath, projectRoot, runTimeoutS, primary.name);
  diagnostics.push(...runResult.diagnostics);
  if (hasError(runResult.diagnostics)) {
    fs.rmSync(buildDir, { recursive: true, force: true });
    respond(requestId, "error", diagnostics);
    return;
  }

  let artifacts;
  try {
    artifacts = writeArtifacts(outputDir, primary.source_path, tcResult.jsPath, mergeResult.merged, runResult.stdoutText, primary.name);
  } catch (e) {
    diagnostics.push(diagnostic("E-TSADAPTER-WRITE-FAILED", `could not write build artifacts: ${e.message}`));
    respond(requestId, "error", diagnostics);
    return;
  } finally {
    fs.rmSync(buildDir, { recursive: true, force: true });
  }

  respond(requestId, "ok", diagnostics, artifacts);
}

function main() {
  const chunks = [];
  process.stdin.on("data", (chunk) => chunks.push(chunk));
  process.stdin.on("end", () => {
    const raw = Buffer.concat(chunks).toString("utf8").split("\n", 1)[0];
    if (!raw || !raw.trim()) {
      log("typescript-backend adapter: no request received on stdin");
      process.exitCode = 1;
      return;
    }
    let request;
    try {
      request = JSON.parse(raw);
    } catch (e) {
      log(`typescript-backend adapter: could not parse request JSON: ${e.message}`);
      process.exitCode = 1;
      return;
    }
    try {
      handle(request);
    } catch (e) {
      log(`typescript-backend adapter: unhandled exception: ${e.stack || e.message}`);
      process.exitCode = 1;
    }
  });
}

main();
