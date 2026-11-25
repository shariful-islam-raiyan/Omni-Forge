"""hello-backend's one service component.

This is the actual backend application OmniForge's python-backend adapter
builds and runs — not a stub. It is invoked as:

    python service.py <run-config.json>

where <run-config.json> holds this component's merged inputs (declared in
component.yaml, overridable by backend/service.json), written by the
adapter before invocation and also copied alongside the built artifacts in
dist/backend/ for inspection.
"""
import json
import sys


def main():
    config = {}
    if len(sys.argv) > 1:
        with open(sys.argv[1], "r", encoding="utf-8") as f:
            config = json.load(f)

    greeting = config.get("greeting", "Hello from the OmniForge python-backend adapter.")
    count = int(config.get("count", 1))

    for i in range(count):
        print(f"[{i + 1}/{count}] {greeting}")


if __name__ == "__main__":
    main()
