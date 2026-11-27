#!/usr/bin/env node
/**
 * OmniForge node-backend adapter.
 *
 * Reads exactly one JSON request line from stdin, writes exactly one JSON
 * response line to stdout. All human-readable logging goes to stderr, never
 * stdout, so it can never corrupt the protocol stream. See
 * docs/adapter-protocol.md for the wire format this implements.
 *
 * Contract for a backend component's script (the file named in a
 * component.yaml's `implementations.backend.source`):
 *
 *   - It is invoked as: <node> <script.js> <run-config.json>
 *   - <run-config.json> is a flat JSON object of the component's merged,
 *     validated inputs: {"input_name": value, ...}.
 *   - It is expected to run to completion and exit (bounded by this
 *     adapter's timeout) rather than behave as a long-running server; its
 *     stdout is captured verbatim as a build artifact (dist/backend/output.txt).
 *   - A non-zero exit code is treated as a build failure.
 *
 * This adapter deliberately uses only Node's built-in modules, so it never
 * needs a package installed beyond the runtime itself.
 */
"use strict";

const fs = require("fs");
const path = require("path");
const cp = require("child_process");

const PROTOCOL_VERSION = "omniforge.dev/adapter-protocol/v1";
const BACKEND_APP_SCHEMA = "omniforge.dev/backend-app/v1";
const ADAPTER_ID = "node-backend";
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

/**
 * Strips the Windows "verbatim" (`\\?\...`) path prefix that
 * `std::fs::canonicalize` on the Rust side produces (every absolute path
 * this adapter receives over the wire protocol -- project_root,
 * output_dir, source_path -- comes from there). A no-op off Windows and on
 * any path not already in that form. Only used right before spawning a
 * *child Node process*, not for this process's own `fs.*` calls (which
 * handle the verbatim form fine) -- Node's own module resolution for a
 * child's main script does not: given a verbatim path whose directory
 * contains a space, `Module._findPath`'s internal `realpathSync` throws
 * `EISDIR` on a path it mis-parses down to just the drive letter. Real bug,
 * reproduced on this project's own `D:\Omni Forge\...` checkout.
 */
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

/** True iff the realpath of `p` is `root` or a descendant of it. */
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

/** Returns {safe, diagnostics}. A component missing its source file or
 * escaping the project root is dropped, with a diagnostic, rather than
 * silently included. */
function validateComponents(components, projectRoot) {
  const diagnostics = [];
  const safe = [];
  for (const component of components) {
    const name = component.name || "<unknown>";
    const sourcePath = component.source_path;
    if (!sourcePath) {
      diagnostics.push(
        diagnostic(
          "E-NODEADAPTER-MISSING-SOURCE",
          `component '${name}' has no resolved source path`,
          "error",
          { component: name }
        )
      );
      continue;
    }
    if (!isWithin(sourcePath, projectRoot)) {
      diagnostics.push(
        diagnostic(
          "E-NODEADAPTER-UNSAFE-PATH",
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
        diagnostic(
          "E-NODEADAPTER-SOURCE-MISSING",
          `component '${name}' source file '${sourcePath}' does not exist`,
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

/** Returns {service, inputs, diagnostics}. diagnostics containing an error
 * means the caller must stop and respond with them. */
function readEntry(entryPath, projectRoot) {
  if (!entryPath) {
    return { service: null, inputs: {}, diagnostics: [] };
  }
  if (!isWithin(entryPath, projectRoot)) {
    return {
      service: null,
      inputs: {},
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-UNSAFE-PATH",
          `entry file '${entryPath}' resolves outside the project root`,
          "error",
          { file: entryPath }
        ),
      ],
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
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-ENTRY-MISSING",
          `entry file '${entryPath}' does not exist`,
          "error",
          { file: entryPath }
        ),
      ],
    };
  }
  let doc;
  try {
    doc = JSON.parse(fs.readFileSync(entryPath, "utf8"));
  } catch (e) {
    return {
      service: null,
      inputs: {},
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-ENTRY-INVALID",
          `could not read/parse entry file '${entryPath}': ${e.message}`,
          "error",
          { file: entryPath }
        ),
      ],
    };
  }
  if (doc.schema !== BACKEND_APP_SCHEMA) {
    return {
      service: null,
      inputs: {},
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-ENTRY-SCHEMA-VERSION",
          `unsupported backend app manifest schema '${doc.schema}', expected '${BACKEND_APP_SCHEMA}'`,
          "error",
          { file: entryPath }
        ),
      ],
    };
  }
  return { service: doc.service || null, inputs: doc.inputs || {}, diagnostics: [] };
}

/** Returns {merged, diagnostics}. Required-but-missing inputs are reported
 * as errors; unknown entry inputs are warnings, not silently dropped. */
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
        diagnostic(
          "W-NODEADAPTER-UNKNOWN-INPUT",
          `input '${inputName}' is not declared on component '${componentName}'`,
          "warning",
          { component: componentName }
        )
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
          "E-NODEADAPTER-MISSING-INPUT",
          `required input '${inputName}' for component '${componentName}' was not provided and has no default`,
          "error",
          { component: componentName }
        )
      );
    }
  }

  return { merged, diagnostics };
}

/** Returns {stdoutText, diagnostics}. diagnostics containing an error means
 * the script did not produce usable output. */
function runBackendScript(sourcePath, configPath, projectRoot, timeoutS, componentName) {
  const timeoutMs = Math.max(1000, timeoutS * 1000);
  let result;
  try {
    result = cp.spawnSync(
      process.execPath,
      [stripVerbatimPrefix(sourcePath), stripVerbatimPrefix(configPath)],
      {
        cwd: stripVerbatimPrefix(projectRoot),
        timeout: timeoutMs,
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
      }
    );
  } catch (e) {
    return {
      stdoutText: null,
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-EXECUTION-FAILED",
          `failed to start backend script '${sourcePath}': ${e.message}`,
          "error",
          { component: componentName }
        ),
      ],
    };
  }

  // A timed-out child reports *both* `result.error` (code `ETIMEDOUT`, since
  // Node's own `spawnSync` treats hitting the `timeout` option as a spawn
  // error) and `result.signal` (how it was actually killed) -- check for
  // that combination before the generic `result.error` branch below, or a
  // real timeout gets misreported as a generic execution failure.
  if ((result.error && result.error.code === "ETIMEDOUT") || result.signal) {
    return {
      stdoutText: null,
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-EXECUTION-TIMEOUT",
          `backend script '${sourcePath}' did not finish within ${timeoutS.toFixed(1)}s`,
          "error",
          { component: componentName }
        ),
      ],
    };
  }

  if (result.error) {
    return {
      stdoutText: null,
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-EXECUTION-FAILED",
          `failed to start backend script '${sourcePath}': ${result.error.message}`,
          "error",
          { component: componentName }
        ),
      ],
    };
  }

  if (result.status !== 0) {
    const tail = (result.stderr || "").slice(-2000);
    return {
      stdoutText: null,
      diagnostics: [
        diagnostic(
          "E-NODEADAPTER-EXECUTION-FAILED",
          `backend script '${sourcePath}' exited with status ${result.status}`,
          "error",
          { component: componentName, explanation: tail || null }
        ),
      ],
    };
  }

  return { stdoutText: result.stdout || "", diagnostics: [] };
}

/** Writes the runnable package + proof-of-execution artifacts inside
 * outputDir and returns their paths relative to it. */
function writeArtifacts(outputDir, sourcePath, mergedInputs, stdoutText, componentName) {
  const artifacts = [];

  const scriptBasename = path.basename(sourcePath);
  const appDir = path.join(outputDir, "app");
  fs.mkdirSync(appDir, { recursive: true });
  const sourceText = fs.readFileSync(sourcePath, "utf8");
  fs.writeFileSync(path.join(appDir, scriptBasename), sourceText, "utf8");
  artifacts.push(`app/${scriptBasename}`);

  fs.writeFileSync(
    path.join(outputDir, "run-config.json"),
    JSON.stringify(mergedInputs, Object.keys(mergedInputs).sort(), 2),
    "utf8"
  );
  artifacts.push("run-config.json");

  fs.writeFileSync(path.join(outputDir, "output.txt"), stdoutText, "utf8");
  artifacts.push("output.txt");

  fs.writeFileSync(
    path.join(outputDir, "run.sh"),
    '#!/bin/sh\nDIR="$(cd "$(dirname "$0")" && pwd)"\n' +
      `exec node "$DIR/app/${scriptBasename}" "$DIR/run-config.json"\n`,
    { encoding: "utf8" }
  );
  artifacts.push("run.sh");

  fs.writeFileSync(
    path.join(outputDir, "run.bat"),
    `@echo off\r\nnode "%~dp0app\\${scriptBasename}" "%~dp0run-config.json"\r\n`,
    { encoding: "utf8" }
  );
  artifacts.push("run.bat");

  const manifest = {
    schema: "omniforge.dev/backend-build-manifest/v1",
    adapter: { id: ADAPTER_ID, version: ADAPTER_VERSION },
    component: componentName,
    artifacts: [...artifacts, "build-manifest.json"].sort(),
  };
  fs.writeFileSync(
    path.join(outputDir, "build-manifest.json"),
    JSON.stringify(manifest, null, 2),
    "utf8"
  );
  artifacts.push("build-manifest.json");

  return artifacts;
}

function handle(request) {
  const requestId = request.request_id || "unknown";

  if (request.protocol !== PROTOCOL_VERSION) {
    respond(requestId, "error", [
      diagnostic(
        "E-NODEADAPTER-PROTOCOL-VERSION",
        `unsupported protocol '${request.protocol}', expected '${PROTOCOL_VERSION}'`
      ),
    ]);
    return;
  }

  const operation = request.operation;
  if (operation !== "validate" && operation !== "compile") {
    respond(requestId, "error", [
      diagnostic(
        "E-NODEADAPTER-UNSUPPORTED-OPERATION",
        `operation '${operation}' is not supported by the ${ADAPTER_ID} adapter`,
        "error",
        { suggestion: "supported operations: validate, compile" }
      ),
    ]);
    return;
  }

  const projectRoot = request.project_root || ".";
  const outputDir = request.output_dir || ".";
  const entryPath = request.entry_path;
  const components = (request.config && request.config.components) || [];

  if (components.length === 0) {
    respond(requestId, "error", [
      diagnostic(
        "E-NODEADAPTER-NO-COMPONENTS",
        "no components were provided to build for the 'backend' target"
      ),
    ]);
    return;
  }

  const { safe: safeComponents, diagnostics } = validateComponents(components, projectRoot);
  if (safeComponents.length === 0) {
    respond(requestId, "error", diagnostics);
    return;
  }

  if (!isWithin(outputDir, projectRoot)) {
    diagnostics.push(
      diagnostic(
        "E-NODEADAPTER-UNSAFE-PATH",
        `output directory '${outputDir}' resolves outside the project root`
      )
    );
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
      diagnostics.push(
        diagnostic(
          "E-NODEADAPTER-SERVICE-NOT-FOUND",
          `entry file names service '${entryResult.service}', which was not resolved for this build`
        )
      );
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

  if (operation === "validate") {
    respond(requestId, "ok", diagnostics);
    return;
  }

  try {
    fs.mkdirSync(outputDir, { recursive: true });
  } catch (e) {
    diagnostics.push(
      diagnostic("E-NODEADAPTER-WRITE-FAILED", `could not create output directory: ${e.message}`)
    );
    respond(requestId, "error", diagnostics);
    return;
  }

  const timeoutS = Math.max(1.0, (request.timeout_ms || 30000) / 1000.0 - 2.0);

  // Write the run-config *before* executing, so the script under test can
  // read it (and so it still exists as an artifact even if the script
  // fails, for debugging).
  const configPath = path.join(outputDir, "run-config.json");
  try {
    fs.writeFileSync(
      configPath,
      JSON.stringify(mergeResult.merged, Object.keys(mergeResult.merged).sort(), 2),
      "utf8"
    );
  } catch (e) {
    diagnostics.push(
      diagnostic("E-NODEADAPTER-WRITE-FAILED", `could not write run-config.json: ${e.message}`)
    );
    respond(requestId, "error", diagnostics);
    return;
  }

  const runResult = runBackendScript(
    primary.source_path,
    configPath,
    projectRoot,
    timeoutS,
    primary.name
  );
  diagnostics.push(...runResult.diagnostics);
  if (hasError(runResult.diagnostics)) {
    respond(requestId, "error", diagnostics);
    return;
  }

  let artifacts;
  try {
    artifacts = writeArtifacts(
      outputDir,
      primary.source_path,
      mergeResult.merged,
      runResult.stdoutText,
      primary.name
    );
  } catch (e) {
    diagnostics.push(
      diagnostic("E-NODEADAPTER-WRITE-FAILED", `could not write build artifacts: ${e.message}`)
    );
    respond(requestId, "error", diagnostics);
    return;
  }

  respond(requestId, "ok", diagnostics, artifacts);
}

function main() {
  const chunks = [];
  process.stdin.on("data", (chunk) => chunks.push(chunk));
  process.stdin.on("end", () => {
    const raw = Buffer.concat(chunks).toString("utf8").split("\n", 1)[0];
    if (!raw || !raw.trim()) {
      log("node-backend adapter: no request received on stdin");
      process.exitCode = 1;
      return;
    }
    let request;
    try {
      request = JSON.parse(raw);
    } catch (e) {
      log(`node-backend adapter: could not parse request JSON: ${e.message}`);
      process.exitCode = 1;
      return;
    }
    try {
      handle(request);
    } catch (e) {
      log(`node-backend adapter: unhandled exception: ${e.stack || e.message}`);
      process.exitCode = 1;
    }
  });
}

main();
