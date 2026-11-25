#!/usr/bin/env python3
"""OmniForge python-backend adapter.

Reads exactly one JSON request line from stdin, writes exactly one JSON
response line to stdout. All human-readable logging goes to stderr, never
stdout, so it can never corrupt the protocol stream. See
docs/adapter-protocol.md for the wire format this implements.

Contract for a backend component's script (the file named in a
component.yaml's `implementations.backend.source`):

  - It is invoked as: <python> <script.py> <run-config.json>
  - <run-config.json> is a flat JSON object of the component's merged,
    validated inputs: {"input_name": value, ...}.
  - It is expected to run to completion and exit (bounded by this
    adapter's timeout) rather than behave as a long-running server; its
    stdout is captured verbatim as a build artifact (dist/backend/output.txt).
  - A non-zero exit code is treated as a build failure.

This adapter deliberately does not import anything outside the Python
standard library, so it never needs a package installed beyond the
interpreter itself.
"""
import json
import os
import subprocess
import sys

PROTOCOL_VERSION = "omniforge.dev/adapter-protocol/v1"
BACKEND_APP_SCHEMA = "omniforge.dev/backend-app/v1"
ADAPTER_ID = "python-backend"
ADAPTER_VERSION = "0.1.0"


def log(message):
    print(message, file=sys.stderr, flush=True)


def diagnostic(code, message, severity="error", **extra):
    entry = {"code": code, "severity": severity, "message": message}
    for key, value in extra.items():
        if value is not None:
            entry[key] = value
    return entry


def respond(request_id, status, diagnostics=None, artifacts=None):
    payload = {
        "protocol": PROTOCOL_VERSION,
        "request_id": request_id,
        "status": status,
        "diagnostics": diagnostics or [],
        "artifacts": artifacts or [],
    }
    sys.stdout.write(json.dumps(payload))
    sys.stdout.write("\n")
    sys.stdout.flush()


def is_within(path, root):
    """True iff the realpath of `path` is `root` or a descendant of it."""
    try:
        path_r = os.path.realpath(path)
        root_r = os.path.realpath(root)
    except OSError:
        return False
    try:
        return os.path.commonpath([path_r, root_r]) == root_r
    except ValueError:
        # e.g. paths on different drives on Windows.
        return False


def has_error(diagnostics):
    return any(d.get("severity") == "error" for d in diagnostics)


def validate_components(components, project_root):
    """Returns (safe_components, diagnostics). A component missing its
    source file or escaping the project root is dropped, with a
    diagnostic, rather than silently included."""
    diagnostics = []
    safe = []
    for component in components:
        name = component.get("name", "<unknown>")
        source_path = component.get("source_path")
        if not source_path:
            diagnostics.append(diagnostic(
                "E-PYADAPTER-MISSING-SOURCE",
                f"component '{name}' has no resolved source path",
                component=name,
            ))
            continue
        if not is_within(source_path, project_root):
            diagnostics.append(diagnostic(
                "E-PYADAPTER-UNSAFE-PATH",
                f"component '{name}' source path '{source_path}' resolves outside the project root",
                component=name,
            ))
            continue
        if not os.path.isfile(source_path):
            diagnostics.append(diagnostic(
                "E-PYADAPTER-SOURCE-MISSING",
                f"component '{name}' source file '{source_path}' does not exist",
                component=name,
                file=source_path,
            ))
            continue
        safe.append(component)
    return safe, diagnostics


def read_entry(entry_path, project_root):
    """Returns (service_name, inputs, diagnostics). diagnostics containing
    an error means the caller must stop and respond with them."""
    if not entry_path:
        return None, {}, []
    if not is_within(entry_path, project_root):
        return None, {}, [diagnostic(
            "E-PYADAPTER-UNSAFE-PATH",
            f"entry file '{entry_path}' resolves outside the project root",
            file=entry_path,
        )]
    if not os.path.isfile(entry_path):
        return None, {}, [diagnostic(
            "E-PYADAPTER-ENTRY-MISSING",
            f"entry file '{entry_path}' does not exist",
            file=entry_path,
        )]
    try:
        with open(entry_path, "r", encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        return None, {}, [diagnostic(
            "E-PYADAPTER-ENTRY-INVALID",
            f"could not read/parse entry file '{entry_path}': {e}",
            file=entry_path,
        )]
    if doc.get("schema") != BACKEND_APP_SCHEMA:
        return None, {}, [diagnostic(
            "E-PYADAPTER-ENTRY-SCHEMA-VERSION",
            f"unsupported backend app manifest schema '{doc.get('schema')}', expected '{BACKEND_APP_SCHEMA}'",
            file=entry_path,
        )]
    return doc.get("service"), (doc.get("inputs") or {}), []


def merge_inputs(interface, entry_inputs, component_name):
    """Returns (merged, diagnostics). Required-but-missing inputs are
    reported as errors; unknown entry inputs are warnings, not silently
    dropped."""
    declared = (interface or {}).get("inputs") or {}
    merged = {}
    diagnostics = []

    for input_name, spec in declared.items():
        if spec.get("default") is not None:
            merged[input_name] = spec["default"]

    for input_name, value in entry_inputs.items():
        if input_name not in declared:
            diagnostics.append(diagnostic(
                "W-PYADAPTER-UNKNOWN-INPUT",
                f"input '{input_name}' is not declared on component '{component_name}'",
                severity="warning",
                component=component_name,
            ))
            continue
        merged[input_name] = value

    for input_name, spec in declared.items():
        if spec.get("required") and input_name not in merged:
            diagnostics.append(diagnostic(
                "E-PYADAPTER-MISSING-INPUT",
                f"required input '{input_name}' for component '{component_name}' "
                "was not provided and has no default",
                component=component_name,
            ))

    return merged, diagnostics


def run_backend_script(source_path, config_path, project_root, timeout_s, component_name):
    """Returns (stdout_text, diagnostics). diagnostics containing an error
    means the script did not produce usable output."""
    try:
        result = subprocess.run(
            [sys.executable, source_path, config_path],
            cwd=project_root,
            capture_output=True,
            text=True,
            timeout=timeout_s,
        )
    except subprocess.TimeoutExpired:
        return None, [diagnostic(
            "E-PYADAPTER-EXECUTION-TIMEOUT",
            f"backend script '{source_path}' did not finish within {timeout_s:.1f}s",
            component=component_name,
        )]
    except OSError as e:
        return None, [diagnostic(
            "E-PYADAPTER-EXECUTION-FAILED",
            f"failed to start backend script '{source_path}': {e}",
            component=component_name,
        )]

    if result.returncode != 0:
        tail = (result.stderr or "")[-2000:]
        return None, [diagnostic(
            "E-PYADAPTER-EXECUTION-FAILED",
            f"backend script '{source_path}' exited with status {result.returncode}",
            component=component_name,
            explanation=tail or None,
        )]

    return result.stdout or "", []


def write_artifacts(output_dir, source_path, merged_inputs, stdout_text, component_name):
    """Writes the runnable package + proof-of-execution artifacts inside
    output_dir and returns their paths relative to it."""
    artifacts = []

    script_basename = os.path.basename(source_path)
    app_dir = os.path.join(output_dir, "app")
    os.makedirs(app_dir, exist_ok=True)
    with open(source_path, "r", encoding="utf-8") as src:
        source_text = src.read()
    with open(os.path.join(app_dir, script_basename), "w", encoding="utf-8") as dst:
        dst.write(source_text)
    artifacts.append(f"app/{script_basename}")

    with open(os.path.join(output_dir, "run-config.json"), "w", encoding="utf-8") as f:
        json.dump(merged_inputs, f, indent=2, sort_keys=True)
    artifacts.append("run-config.json")

    with open(os.path.join(output_dir, "output.txt"), "w", encoding="utf-8") as f:
        f.write(stdout_text)
    artifacts.append("output.txt")

    with open(os.path.join(output_dir, "run.sh"), "w", encoding="utf-8", newline="\n") as f:
        f.write("#!/bin/sh\n")
        f.write('DIR="$(cd "$(dirname "$0")" && pwd)"\n')
        f.write(f'exec python3 "$DIR/app/{script_basename}" "$DIR/run-config.json"\n')
    artifacts.append("run.sh")

    with open(os.path.join(output_dir, "run.bat"), "w", encoding="utf-8", newline="\r\n") as f:
        f.write("@echo off\r\n")
        f.write(f'python "%~dp0app\\{script_basename}" "%~dp0run-config.json"\r\n')
    artifacts.append("run.bat")

    manifest = {
        "schema": "omniforge.dev/backend-build-manifest/v1",
        "adapter": {"id": ADAPTER_ID, "version": ADAPTER_VERSION},
        "component": component_name,
        "artifacts": sorted(artifacts + ["build-manifest.json"]),
    }
    with open(os.path.join(output_dir, "build-manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)
    artifacts.append("build-manifest.json")

    return artifacts


def handle(request):
    request_id = request.get("request_id", "unknown")

    if request.get("protocol") != PROTOCOL_VERSION:
        respond(request_id, "error", [diagnostic(
            "E-PYADAPTER-PROTOCOL-VERSION",
            f"unsupported protocol '{request.get('protocol')}', expected '{PROTOCOL_VERSION}'",
        )])
        return

    operation = request.get("operation")
    if operation not in ("validate", "compile"):
        respond(request_id, "error", [diagnostic(
            "E-PYADAPTER-UNSUPPORTED-OPERATION",
            f"operation '{operation}' is not supported by the {ADAPTER_ID} adapter",
            suggestion="supported operations: validate, compile",
        )])
        return

    project_root = request.get("project_root") or "."
    output_dir = request.get("output_dir") or "."
    entry_path = request.get("entry_path")
    components = (request.get("config") or {}).get("components") or []

    if not components:
        respond(request_id, "error", [diagnostic(
            "E-PYADAPTER-NO-COMPONENTS",
            "no components were provided to build for the 'backend' target",
        )])
        return

    safe_components, diagnostics = validate_components(components, project_root)
    if not safe_components:
        respond(request_id, "error", diagnostics)
        return

    if not is_within(output_dir, project_root):
        diagnostics.append(diagnostic(
            "E-PYADAPTER-UNSAFE-PATH",
            f"output directory '{output_dir}' resolves outside the project root",
        ))
        respond(request_id, "error", diagnostics)
        return

    entry_service, entry_inputs, entry_diagnostics = read_entry(entry_path, project_root)
    diagnostics.extend(entry_diagnostics)
    if has_error(entry_diagnostics):
        respond(request_id, "error", diagnostics)
        return

    if entry_service:
        primary = next((c for c in safe_components if c["name"] == entry_service), None)
        if primary is None:
            diagnostics.append(diagnostic(
                "E-PYADAPTER-SERVICE-NOT-FOUND",
                f"entry file names service '{entry_service}', which was not resolved for this build",
            ))
            respond(request_id, "error", diagnostics)
            return
    else:
        primary = safe_components[-1]

    merged_inputs, merge_diagnostics = merge_inputs(primary.get("interface"), entry_inputs, primary["name"])
    diagnostics.extend(merge_diagnostics)
    if has_error(merge_diagnostics):
        respond(request_id, "error", diagnostics)
        return

    if operation == "validate":
        respond(request_id, "ok", diagnostics)
        return

    try:
        os.makedirs(output_dir, exist_ok=True)
    except OSError as e:
        diagnostics.append(diagnostic("E-PYADAPTER-WRITE-FAILED", f"could not create output directory: {e}"))
        respond(request_id, "error", diagnostics)
        return

    timeout_s = max(1.0, (request.get("timeout_ms", 30000) / 1000.0) - 2.0)

    # Write the run-config *before* executing, so the script under test
    # can read it (and so it still exists as an artifact even if the
    # script fails, for debugging).
    config_path = os.path.join(output_dir, "run-config.json")
    try:
        with open(config_path, "w", encoding="utf-8") as f:
            json.dump(merged_inputs, f, indent=2, sort_keys=True)
    except OSError as e:
        diagnostics.append(diagnostic("E-PYADAPTER-WRITE-FAILED", f"could not write run-config.json: {e}"))
        respond(request_id, "error", diagnostics)
        return

    stdout_text, run_diagnostics = run_backend_script(
        primary["source_path"], config_path, project_root, timeout_s, primary["name"]
    )
    diagnostics.extend(run_diagnostics)
    if has_error(run_diagnostics):
        respond(request_id, "error", diagnostics)
        return

    try:
        artifacts = write_artifacts(output_dir, primary["source_path"], merged_inputs, stdout_text, primary["name"])
    except OSError as e:
        diagnostics.append(diagnostic("E-PYADAPTER-WRITE-FAILED", f"could not write build artifacts: {e}"))
        respond(request_id, "error", diagnostics)
        return

    respond(request_id, "ok", diagnostics, artifacts)


def main():
    raw = sys.stdin.readline()
    if not raw.strip():
        log("python-backend adapter: no request received on stdin")
        return 1
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as e:
        log(f"python-backend adapter: could not parse request JSON: {e}")
        return 1
    handle(request)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # last-resort guard: never crash silently.
        log(f"python-backend adapter: unhandled exception: {exc}")
        sys.exit(1)
