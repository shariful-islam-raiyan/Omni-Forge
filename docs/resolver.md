# Component resolver

`crates/omni-core/src/resolver.rs` + `crates/omni-core/src/graph.rs`.

## Inputs

- All discovered `ComponentManifest`s for the project.
- All registered adapters' `AdapterManifest`s.
- A target name (e.g. `"web"`).
- A list of **root** component names. The orchestrator
  (`crates/omni-cli/src/orchestrator.rs::build_target`) computes roots as
  every discovered component that declares an implementation for the
  requested target — a component meant only for another target is simply
  not a root for this one, so it can never produce a spurious
  "no implementation for target X" error for a target it was never meant
  to support.

## Algorithm

1. Breadth-first walk from the roots, following `dependencies`, building a
   `DependencyGraph` edge for every `(component, dependency)` pair.
2. For each visited component, in order:
   - must exist (`E-RESOLVE-MISSING-COMPONENT`);
   - must have an implementation for the target
     (`E-RESOLVE-NO-IMPLEMENTATION`, listing which targets it *does*
     support);
   - that implementation's adapter must exist
     (`E-RESOLVE-NO-ADAPTER`);
   - that adapter must declare the target, the component's kind, and the
     `compile` operation (`E-RESOLVE-CAPABILITY`, one diagnostic per
     missing capability).
3. If every component resolved cleanly, the graph is topologically sorted
   (`DependencyGraph::topological_order`). A cycle produces
   `E-RESOLVE-CYCLE` with the actual cycle path (e.g. `a -> b -> a`), not
   just "a cycle exists somewhere."
4. The result is a `ResolvedBuildPlan` — a dependency-first-ordered list of
   `ResolvedComponent` (name, version, kind, chosen implementation,
   manifest directory, and the full adapter manifest it resolved to).

## Determinism

- Component discovery sorts by directory name, then the result list is
  sorted by component name.
- The dependency graph uses `BTreeMap`/`BTreeSet` throughout, so iteration
  order never depends on hash-map iteration order or insertion order.
- Given the same manifests, `resolve()` always returns components in the
  same order. `crates/omni-core/src/graph.rs` has a test
  (`deterministic_ordering_for_independent_nodes`) asserting this
  explicitly for nodes with no ordering constraint between them.

## Explaining failure (and success)

Every failure path names: the component, the target, and — for a missing
implementation — the targets that *are* available, so the fix is obvious
without re-reading the component manifest. Example, verbatim from the
test suite:

```
error E-RESOLVE-NO-IMPLEMENTATION: component 'ui.button' has no implementation for target 'web'. Available: [android]
  suggestion: add an implementation for target 'web', or disable that target
```

There is currently no separate "explain why this implementation (as
opposed to another) was chosen" report, because the component manifest
format only allows one implementation per target — there is never a
choice to explain, only a presence/absence check. If/when the format
grows multiple candidate implementations per target (see roadmap), this
document will describe the selection rule and this gap will close.
