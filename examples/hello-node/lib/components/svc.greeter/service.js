/**
 * hello-node's one service component.
 *
 * This is the actual backend application OmniForge's node-backend adapter
 * builds and runs — not a stub. It is invoked as:
 *
 *     node service.js <run-config.json>
 *
 * where <run-config.json> holds this component's merged inputs (declared in
 * component.yaml, overridable by backend/service.json), written by the
 * adapter before invocation and also copied alongside the built artifacts in
 * dist/backend/ for inspection.
 */
"use strict";

const fs = require("fs");

function main() {
  let config = {};
  if (process.argv.length > 2) {
    config = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
  }

  const greeting = config.greeting || "Hello from the OmniForge node-backend adapter.";
  const count = Number(config.count || 1);

  for (let i = 0; i < count; i++) {
    console.log(`[${i + 1}/${count}] ${greeting}`);
  }
}

main();
