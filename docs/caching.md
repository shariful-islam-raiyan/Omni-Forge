# Build cache

`crates/omni-core/src/cache.rs`. A minimal, correct, local, content-addressed
cache — not a distributed/remote cache, and not incremental below the
target level. Enabled by default (`build.cache: true` in `omni.yaml`);
disabling it sets every build's report `cache` field to `disabled` and
always runs the adapter.

## Storage

`<project_root>/.omniforge/cache/<target>/<key>/` — a full copy of that
target's output tree for that key, written by `BuildCache::store` and
restored verbatim by `BuildCache::restore` (both via a symlink-skipping
recursive copy, `omni_core::fsutil::copy_dir_all`, so a cache entry can
never smuggle a link pointing outside the sandboxed directory). Not
touched by `omni clean` (clean only removes `build.output`, i.e. `dist/`),
so the cache survives a clean on purpose — it's a cache of derived output,
not build output itself.

## Cache key (`orchestrator::compute_cache_key`)

A single SHA-256 over, in a fixed order, via `CacheKeyBuilder` (which
length-prefixes every chunk so `"ab"+"c"` can never hash the same as
`"a"+"bc"`):

1. target name, adapter id, adapter version;
2. the target's entry file's raw bytes, if it has one;
3. the target's declared `contract` file's path and raw bytes, if
   `omni.yaml` sets one (`targets.<name>.contract`) — added in the
   "Unified Application Runtime" milestone: previously the contract file
   was not part of the key at all, so editing it did not invalidate a
   backend target's cache (see
   `crates/omni-cli/tests/fullstack_http.rs::editing_the_contract_busts_the_backend_build_cache`
   for the regression test);
4. for every resolved component, in name-sorted order: its name, version,
   full `component.yaml` bytes, and its implementation source file's
   bytes;
5. every file under the project's `public/` directory, recursively, sorted
   by relative path — path and content both.

This is **deliberately coarser than per-file dependency tracking**: *any*
change under `public/` invalidates the cache for *every* target, even one
that doesn't reference the changed file. That trade-off was made to keep
the key-computation code simple and unambiguously correct (no risk of
missing an asset a future adapter might read without declaring it) rather
than building a precise per-file graph for a single adapter. A future
adapter that only cares about declared assets could narrow this —
documented as Planned, not pretended to already exist.

No secret values, environment variables, or absolute machine-specific
paths are ever hashed into the key — only project-relative file content
and manifest-declared identifiers.

## Guarantees actually tested

`crates/omni-core/src/cache.rs` unit tests:
- identical inputs → identical key;
- any difference → a different key;
- the length-prefixing scheme specifically defeats concatenation
  ambiguity;
- store → restore round-trips file content exactly;
- a miss (`lookup` on an absent key) returns `None`/`false` rather than
  panicking or fabricating an empty hit.

`crates/omni-cli/tests/cli.rs::create_check_build_clean_round_trip` proves
the end-to-end behavior for `web`: a second `omni build web` with no
changes reports `cache hit`. The cache key computation is entirely
target-agnostic (it only reads `component.yaml`s, implementation source
files, the entry file, and `public/`), and this was verified manually for
the `backend` target too: editing `examples/hello-backend/backend/
service.json`'s `count` field and rebuilding produced a cache **miss**,
the python-backend adapter re-ran, and `dist/backend/output.txt` reflected
the new value — confirming the cache doesn't serve a stale out-of-process
adapter's output any more than it would an in-process one's.
