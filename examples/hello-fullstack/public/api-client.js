// A small, real browser-side API client for the hello-fullstack example.
//
// This file is shipped verbatim by the web-static adapter (see
// web/app.yaml's `public.js` list, consumed by
// crates/omni-web-adapter/src/lib.rs, which copies declared JS/CSS/asset
// files byte-for-byte and emits a <script defer> tag for each one -- no
// adapter code was changed to support this). OmniForge itself never
// executes or interprets this script; it is only ever run by a real
// browser once dist/web/index.html is opened. See
// docs/decisions/0003-template-only-rendering.md for why the renderer has
// no JS engine of its own, and examples/hello-fullstack/README.md for why
// this capability is documented and manually verified rather than covered
// by an automated browser test in this milestone's CI.
(function () {
  "use strict";

  var app = document.getElementById("app");
  if (!app) {
    return;
  }
  var status = document.getElementById("status");
  var apiBase = app.getAttribute("data-api-base") || "";

  fetch(apiBase + "/api/greeting")
    .then(function (response) {
      if (!response.ok) {
        throw new Error("backend responded with HTTP " + response.status);
      }
      return response.json();
    })
    .then(function (data) {
      status.textContent =
        data.greeting + " (requested " + data.count + " time(s))";
    })
    .catch(function (err) {
      status.textContent =
        "Could not reach the backend at " + apiBase + ": " + err.message;
    });
})();
