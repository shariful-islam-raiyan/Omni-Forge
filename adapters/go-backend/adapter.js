#!/usr/bin/env node
/**
 * OmniForge go-backend adapter.
 *
 * Structurally similar to node-backend/typescript-backend's adapter.js
 * (same wire protocol, same path-safety checks, same input merging), but
 * the component's source is real Go, genuinely compiled by a real `go
 * build` into a real, standalone native executable -- not run through an
 * interpreter at all. `validate` runs `go build` to a discarded temp
 * output (real compile-check, writes nothing lasting). `compile` builds
 * the real executable into this build's `app/` output and then runs it
 * directly. See docs/decisions/0015-go-backend-adapter.md.
 *
 * Uses only Node's built-in modules for its own logic.
 */
"use strict";

const fs = require("fs");
const path = require("path");
const os = require("os");
const cp = require("child_process");

const PROTOCOL_VERSION = "omniforge.dev/adapter-protocol/v1";
const BACKEND_APP_SCHEMA = "omniforge.dev/backend-app/v1";
const ADAPTER_ID = "go-backend";
const ADAPTER_VERSION = "0.1.0";

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
        diagnostic("E-GOADAPTER-MISSING-SOURCE", `component '${name}' has no resolved source path`, "error", { component: name })
      );
      continue;
    }
    if (!isWithin(sourcePath, projectRoot)) {
      diagnostics.push(
        diagnostic(
          "E-GOADAPTER-UNSAFE-PATH",
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
        diagnostic("E-GOADAPTER-SOURCE-MISSING", `component '${name}' source file '${sourcePath}' does not exist`, "error", {
          component: name,
          file: sourcePath,
        })
      );
      continue;
    }
    if (!sourcePath.toLowerCase().endsWith(".go")) {
      diagnostics.push(
        diagnostic("E-GOADAPTER-NOT-GO-SOURCE", `component '${name}' source '${sourcePath}' is not a .go file`, "error", {
          component: name,
          file: sourcePath,
        })
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
      diagnostics: [diagnostic("E-GOADAPTER-UNSAFE-PATH", `entry file '${entryPath}' resolves outside the project root`, "error", { file: entryPath })],
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
      diagnostics: [diagnostic("E-GOADAPTER-ENTRY-MISSING", `entry file '${entryPath}' does not exist`, "error", { file: entryPath })],
    };
  }
  let doc;
  try {
    doc = JSON.parse(fs.readFileSync(entryPath, "utf8"));
  } catch (e) {
    return {
      service: null,
      inputs: {},
      diagnostics: [diagnostic("E-GOADAPTER-ENTRY-INVALID", `could not read/parse entry file '${entryPath}': ${e.message}`, "error", { file: entryPath })],
    };
  }
  if (doc.schema !== BACKEND_APP_SCHEMA) {
    return {
      service: null,
      inputs: {},
      diagnostics: [
        diagnostic(
          "E-GOADAPTER-ENTRY-SCHEMA-VERSION",
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
        diagnostic("W-GOADAPTER-UNKNOWN-INPUT", `input '${inputName}' is not declared on component '${componentName}'`, "warning", {
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
          "E-GOADAPTER-MISSING-INPUT",
          `required input '${inputName}' for component '${componentName}' was not provided and has no default`,
          "error",
          { component: componentName }
        )
      );
    }
  }
  return { merged, diagnostics };
}

function exeName(baseName) {
  return process.platform === "win32" ? `${baseName}.exe` : baseName;
}

/** Runs the real `go build`. `outBinary` is the exact file `go build -o`
 * should produce; the caller deletes it afterward for `validate`
 * (type/compile-check only, no lasting artifact) or keeps it for
 * `compile`. Returns {diagnostics}; non-empty+error means the build
 * failed -- real `go build` compiler output captured as `explanation`. */
function runGoBuild(sourcePath, outBinary, timeoutMs, componentName) {
  let result;
  try {
    result = cp.spawnSync(
      "go",
      ["build", "-o", stripVerbatimPrefix(outBinary), stripVerbatimPrefix(sourcePath)],
      {
        timeout: timeoutMs,
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
        env: Object.assign({}, process.env, { GO111MODULE: "off" }),
      }
    );
  } catch (e) {
    return [diagnostic("E-GOADAPTER-BUILD-FAILED", `failed to start go build: ${e.message}`, "error", { component: componentName })];
  }
  if ((result.error && result.error.code === "ETIMEDOUT") || result.signal) {
    return [diagnostic("E-GOADAPTER-BUILD-TIMEOUT", `go build did not finish compiling '${sourcePath}' in time`, "error", { component: componentName })];
  }
  if (result.error) {
    return [diagnostic("E-GOADAPTER-BUILD-FAILED", `failed to start go build: ${result.error.message}`, "error", { component: componentName })];
  }
  if (result.status !== 0) {
    const tail = ((result.stdout || "") + (result.stderr || "")).slice(-4000);
    return [
      diagnostic("E-GOADAPTER-BUILD-FAILED", `go build reported compile errors for '${sourcePath}'`, "error", {
        component: componentName,
        explanation: tail || null,
      }),
    ];
  }
  if (!fs.existsSync(outBinary)) {
    return [diagnostic("E-GOADAPTER-BUILD-FAILED", `go build exited successfully but did not produce '${outBinary}'`, "error", { component: componentName })];
  }
  return [];
}

function runBackendBinary(binaryPath, configPath, projectRoot, timeoutS, componentName) {
  const timeoutMs = Math.round(Math.max(1000, timeoutS * 1000));
  let result;
  try {
    result = cp.spawnSync(stripVerbatimPrefix(binaryPath), [stripVerbatimPrefix(configPath)], {
      cwd: stripVerbatimPrefix(projectRoot),
      timeout: timeoutMs,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    });
  } catch (e) {
    return { stdoutText: null, diagnostics: [diagnostic("E-GOADAPTER-EXECUTION-FAILED", `failed to start compiled binary '${binaryPath}': ${e.message}`, "error", { component: componentName })] };
  }
  if ((result.error && result.error.code === "ETIMEDOUT") || result.signal) {
    return {
      stdoutText: null,
      diagnostics: [diagnostic("E-GOADAPTER-EXECUTION-TIMEOUT", `compiled binary '${binaryPath}' did not finish within ${timeoutS.toFixed(1)}s`, "error", { component: componentName })],
    };
  }
  if (result.error) {
    return { stdoutText: null, diagnostics: [diagnostic("E-GOADAPTER-EXECUTION-FAILED", `failed to start compiled binary '${binaryPath}': ${result.error.message}`, "error", { component: componentName })] };
  }
  if (result.status !== 0) {
    const tail = (result.stderr || "").slice(-2000);
    return {
      stdoutText: null,
      diagnostics: [
        diagnostic("E-GOADAPTER-EXECUTION-FAILED", `compiled binary '${binaryPath}' exited with status ${result.status}`, "error", {
          component: componentName,
          explanation: tail || null,
        }),
      ],
    };
  }
  return { stdoutText: result.stdout || "", diagnostics: [] };
}

function writeArtifacts(outputDir, sourcePath, binaryPath, mergedInputs, stdoutText, componentName) {
  const artifacts = [];
  const binBasename = path.basename(binaryPath);
  const appDir = path.join(outputDir, "app");
  fs.mkdirSync(appDir, { recursive: true });
  fs.copyFileSync(binaryPath, path.join(appDir, binBasename));
  if (process.platform !== "win32") {
    fs.chmodSync(path.join(appDir, binBasename), 0o755);
  }
  artifacts.push(`app/${binBasename}`);

  fs.copyFileSync(sourcePath, path.join(outputDir, "source.go"));
  artifacts.push("source.go");

  fs.writeFileSync(path.join(outputDir, "run-config.json"), JSON.stringify(mergedInputs, Object.keys(mergedInputs).sort(), 2), "utf8");
  artifacts.push("run-config.json");

  fs.writeFileSync(path.join(outputDir, "output.txt"), stdoutText, "utf8");
  artifacts.push("output.txt");

  fs.writeFileSync(
    path.join(outputDir, "run.sh"),
    '#!/bin/sh\nDIR="$(cd "$(dirname "$0")" && pwd)"\n' + `exec "$DIR/app/${binBasename}" "$DIR/run-config.json"\n`,
    { encoding: "utf8" }
  );
  artifacts.push("run.sh");

  fs.writeFileSync(path.join(outputDir, "run.bat"), `@echo off\r\n"%~dp0app\\${binBasename}" "%~dp0run-config.json"\r\n`, { encoding: "utf8" });
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

function handle(request) {
  const requestId = request.request_id || "unknown";

  if (request.protocol !== PROTOCOL_VERSION) {
    respond(requestId, "error", [diagnostic("E-GOADAPTER-PROTOCOL-VERSION", `unsupported protocol '${request.protocol}', expected '${PROTOCOL_VERSION}'`)]);
    return;
  }

  const operation = request.operation;
  if (operation !== "validate" && operation !== "compile") {
    respond(requestId, "error", [
      diagnostic("E-GOADAPTER-UNSUPPORTED-OPERATION", `operation '${operation}' is not supported by the ${ADAPTER_ID} adapter`, "error", {
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
    respond(requestId, "error", [diagnostic("E-GOADAPTER-NO-COMPONENTS", "no components were provided to build for the 'backend' target")]);
    return;
  }

  const { safe: safeComponents, diagnostics } = validateComponents(components, projectRoot);
  if (safeComponents.length === 0) {
    respond(requestId, "error", diagnostics);
    return;
  }

  if (!isWithin(outputDir, projectRoot)) {
    diagnostics.push(diagnostic("E-GOADAPTER-UNSAFE-PATH", `output directory '${outputDir}' resolves outside the project root`));
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
      diagnostics.push(diagnostic("E-GOADAPTER-SERVICE-NOT-FOUND", `entry file names service '${entryResult.service}', which was not resolved for this build`));
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

  const totalBudgetS = Math.max(5.0, (request.timeout_ms || 60000) / 1000.0 - 2.0);
  const buildTimeoutMs = Math.round(Math.max(5000, totalBudgetS * 1000 * 0.85));
  const runTimeoutS = Math.max(1.0, totalBudgetS * 0.15);

  if (operation === "validate") {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "omniforge-go-"));
    const tempBinary = path.join(tempDir, exeName("validate"));
    const buildDiags = runGoBuild(primary.source_path, tempBinary, buildTimeoutMs, primary.name);
    fs.rmSync(tempDir, { recursive: true, force: true });
    diagnostics.push(...buildDiags);
    respond(requestId, hasError(buildDiags) ? "error" : "ok", diagnostics);
    return;
  }

  try {
    fs.mkdirSync(outputDir, { recursive: true });
  } catch (e) {
    diagnostics.push(diagnostic("E-GOADAPTER-WRITE-FAILED", `could not create output directory: ${e.message}`));
    respond(requestId, "error", diagnostics);
    return;
  }

  const buildDir = fs.mkdtempSync(path.join(os.tmpdir(), "omniforge-go-"));
  const binaryPath = path.join(buildDir, exeName(path.basename(primary.source_path, ".go")));
  const buildDiags = runGoBuild(primary.source_path, binaryPath, buildTimeoutMs, primary.name);
  diagnostics.push(...buildDiags);
  if (hasError(buildDiags)) {
    fs.rmSync(buildDir, { recursive: true, force: true });
    respond(requestId, "error", diagnostics);
    return;
  }

  const configPath = path.join(outputDir, "run-config.json");
  try {
    fs.writeFileSync(configPath, JSON.stringify(mergeResult.merged, Object.keys(mergeResult.merged).sort(), 2), "utf8");
  } catch (e) {
    fs.rmSync(buildDir, { recursive: true, force: true });
    diagnostics.push(diagnostic("E-GOADAPTER-WRITE-FAILED", `could not write run-config.json: ${e.message}`));
    respond(requestId, "error", diagnostics);
    return;
  }

  const runResult = runBackendBinary(binaryPath, configPath, projectRoot, runTimeoutS, primary.name);
  diagnostics.push(...runResult.diagnostics);
  if (hasError(runResult.diagnostics)) {
    fs.rmSync(buildDir, { recursive: true, force: true });
    respond(requestId, "error", diagnostics);
    return;
  }

  let artifacts;
  try {
    artifacts = writeArtifacts(outputDir, primary.source_path, binaryPath, mergeResult.merged, runResult.stdoutText, primary.name);
  } catch (e) {
    diagnostics.push(diagnostic("E-GOADAPTER-WRITE-FAILED", `could not write build artifacts: ${e.message}`));
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
      log("go-backend adapter: no request received on stdin");
      process.exitCode = 1;
      return;
    }
    let request;
    try {
      request = JSON.parse(raw);
    } catch (e) {
      log(`go-backend adapter: could not parse request JSON: ${e.message}`);
      process.exitCode = 1;
      return;
    }
    try {
      handle(request);
    } catch (e) {
      log(`go-backend adapter: unhandled exception: ${e.stack || e.message}`);
      process.exitCode = 1;
    }
  });
}

main();
