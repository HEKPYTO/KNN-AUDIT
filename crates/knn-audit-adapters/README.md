# knn-audit-adapters

Engine contract and the in-process exact control. This crate never depends on
`knn-audit`.

## Contract

`Engine` has `name`, `version`, `prepare`, `search`, `capabilities` and a
default `reset`. `prepare` replaces the collection and is idempotent; a count
mismatch is an `AdapterError`. `search` returns `Hit { id, distance }` strings
in engine order, never re-sorting ties. `CollectionView` carries numeric ids
and a flat `f32` vector buffer. `Caps` declares exact mode and distance metric.

## Modules

| Module | Responsibility |
|---|---|
| `exact` | Certified f64 squared-L2 fast path, dyadic-rational `BigInt` comparison for near-boundary candidates, and exact top-k with smaller-id tie-break. |
| `reference` | In-process engine backed by the exact kernel. It reports `SquaredL2`, exact mode, and no f64-derived ordering decision. |

## Tests

Unit tests define a deterministic fake engine that checks idempotent prepare,
count mismatch, k above collection size, exact-mode failure and round-trip of
non-ASCII id strings. Integration tests cover kernel edge cases (zero, negative
zero, subnormal, absorption, overflow, ties) and a seeded random f64 interval
oracle.
