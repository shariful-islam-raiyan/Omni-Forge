"""hello-fullstack's backend service component.

Dual-mode entry point:

  - Default, matching every other python-backend component in this repo:
    `python service.py <run-config.json>` prints the configured greeting
    `count` times and exits 0. This is the exact one-shot contract
    `adapters/python-backend/adapter.py` drives during `omni build backend`
    (see docs/component-specification.md) -- nothing about that path is
    different from examples/hello-backend.

  - `python service.py <run-config.json> --serve <port>` additionally
    starts a real stdlib HTTP server on 127.0.0.1:<port>, implementing the
    two endpoints declared in backend/api-contract.json: GET /api/greeting
    and POST /api/echo. This mode is never invoked by `omni build`/`omni
    check`/`omni dev` -- it exists so this example has a real, runnable
    backend to point a browser or an integration test at. See
    examples/hello-fullstack/README.md for how to start it by hand, and
    docs/decisions/0007-backend-dev-server-is-example-scoped.md for why
    this is a documented convention of this one script rather than a new
    generic adapter operation.

Exactly one line -- "LISTENING <port>" -- is written to stdout once the
server is bound, and stdout is never written to again after that; all
further logging goes to stderr. This mirrors the adapter protocol's own
rule (machine-readable output stays on one dedicated channel) even though
this server does not itself speak the OmniForge adapter protocol.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def load_config(config_path):
    if not config_path:
        return {}
    with open(config_path, "r", encoding="utf-8") as f:
        return json.load(f)


def run_once(config):
    greeting = config.get("greeting", "Hello from the OmniForge hello-fullstack backend.")
    count = int(config.get("count", 1))
    for i in range(count):
        print(f"[{i + 1}/{count}] {greeting}")


class Handler(BaseHTTPRequestHandler):
    config = {}

    def log_message(self, fmt, *args):
        # BaseHTTPRequestHandler logs to stderr by default already, but be
        # explicit: machine-readable output never belongs on stdout here.
        sys.stderr.write(
            "%s - - [%s] %s\n" % (self.address_string(), self.log_date_time_string(), fmt % args)
        )

    def _send_cors_headers(self):
        # The web target is served from a different port (a different
        # origin, by the browser's definition) than this backend, so a
        # real browser enforces CORS on every cross-origin fetch --
        # including a "simple" GET with no custom headers, which the
        # browser still blocks *script access to the response* for without
        # this header, surfacing to `fetch()` as a generic network error.
        # `*` is fine for a local dev example with no cookies/credentials
        # involved; a production backend would echo a specific allowed
        # origin instead.
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def do_OPTIONS(self):
        # A browser sends a preflight OPTIONS request before a POST whose
        # Content-Type is "application/json" (not a CORS "simple" content
        # type) -- without a handler for it, the real POST request below
        # never gets sent at all.
        self.send_response(204)
        self._send_cors_headers()
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _send_json(self, status, payload):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self._send_cors_headers()
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/api/greeting":
            greeting = self.config.get(
                "greeting", "Hello from the OmniForge hello-fullstack backend."
            )
            count = int(self.config.get("count", 1))
            self._send_json(200, {"greeting": greeting, "count": count})
            return
        self._send_json(404, {"error": "not found", "path": self.path})

    def do_POST(self):
        if self.path != "/api/echo":
            self._send_json(404, {"error": "not found", "path": self.path})
            return
        length = int(self.headers.get("Content-Length", 0) or 0)
        raw = self.rfile.read(length) if length > 0 else b""
        try:
            body = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            self._send_json(400, {"error": "request body is not valid JSON"})
            return
        if not isinstance(body, dict) or not isinstance(body.get("message"), str):
            self._send_json(
                400,
                {"error": "request body must be a JSON object with a string 'message' field"},
            )
            return
        self._send_json(200, {"message": body["message"]})


def serve(config, port):
    Handler.config = config
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    bound_port = server.server_address[1]
    print(f"LISTENING {bound_port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


def main():
    args = sys.argv[1:]
    config_path = args[0] if args and not args[0].startswith("--") else None
    config = load_config(config_path)

    if "--serve" in args:
        port = int(args[args.index("--serve") + 1])
        serve(config, port)
        return

    run_once(config)


if __name__ == "__main__":
    main()
