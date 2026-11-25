import { expect, test } from "@playwright/test";

// Real-browser verification of examples/hello-fullstack, driven by a real
// `omni dev` process (see playwright.config.ts's webServer). Nothing here
// is mocked: the page is the real dist/web/index.html the web-static
// adapter rendered, api-client.js is the real shipped script, and the
// backend is the real Python process started by `omni dev`.

test("the page loads and real client-side fetch renders a real backend response", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("h1")).toHaveText("Hello, full-stack OmniForge!");

  const status = page.locator("#status");
  // api-client.js's fetch to /api/greeting is asynchronous; wait for it to
  // actually resolve and replace the "Loading..." placeholder rather than
  // asserting on a fixed timeout.
  await expect(status).not.toHaveText(/Loading greeting/, { timeout: 10_000 });

  const text = await status.textContent();
  expect(text).toMatch(/Hello from hello-fullstack, built by OmniForge!/);
  expect(text).toMatch(/requested 1 time\(s\)/);
});

test("a real browser-executed fetch to the backend's POST /api/echo round-trips correctly", async ({
  page,
}) => {
  await page.goto("/");

  const apiBase = await page.locator("#app").getAttribute("data-api-base");
  expect(apiBase).toBeTruthy();

  // Exercises the real backend's echo endpoint from inside the real page's
  // JS execution context (real fetch, real same-origin-policy-adjacent
  // network stack) -- there is no dedicated echo form in this example's UI
  // today (see lib/components/ui.home/home.html.tmpl), so this drives the
  // same client/server boundary api-client.js uses, directly.
  const result = await page.evaluate(async (base) => {
    const response = await fetch(`${base}/api/echo`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message: "hello from a real browser" }),
    });
    return { status: response.status, body: await response.json() };
  }, apiBase);

  expect(result.status).toBe(200);
  expect(result.body).toEqual({ message: "hello from a real browser" });
});

test("an invalid POST /api/echo request gets the declared 400 shape from a real browser fetch", async ({
  page,
}) => {
  await page.goto("/");
  const apiBase = await page.locator("#app").getAttribute("data-api-base");

  const result = await page.evaluate(async (base) => {
    const response = await fetch(`${base}/api/echo`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    return { status: response.status, body: await response.json() };
  }, apiBase);

  expect(result.status).toBe(400);
  expect(result.body.error).toBeTruthy();
});
