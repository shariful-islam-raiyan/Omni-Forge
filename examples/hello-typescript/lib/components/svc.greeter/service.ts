/**
 * hello-typescript's one service component.
 *
 * Genuinely type-checked and compiled by a real `tsc` before this ever
 * runs (see the typescript-backend adapter and
 * docs/decisions/0013-typescript-backend-adapter.md) -- not merely
 * stripped of type annotations. It is invoked (after compilation) as:
 *
 *     node service.js <run-config.json>
 *
 * where <run-config.json> holds this component's merged inputs.
 */

interface GreeterConfig {
  greeting?: string;
  count?: number;
}

function loadConfig(): GreeterConfig {
  if (process.argv.length > 2) {
    const fs = require("fs");
    return JSON.parse(fs.readFileSync(process.argv[2], "utf8")) as GreeterConfig;
  }
  return {};
}

function main(): void {
  const config: GreeterConfig = loadConfig();
  const greeting: string = config.greeting ?? "Hello from the OmniForge typescript-backend adapter.";
  const count: number = Number(config.count ?? 1);

  for (let i = 0; i < count; i++) {
    console.log(`[${i + 1}/${count}] ${greeting}`);
  }
}

main();
