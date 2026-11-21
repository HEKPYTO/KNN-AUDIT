# knn-audit-adapters
Engine contract and the in-process exact control. This crate never depends on
`knn-audit`.

`qdrant` is a blocking REST adapter using Qdrant's exact search with plain L2.
`pgvector` is a synchronous Postgres adapter that loads vectors with `COPY`,
disables index scans on every pooled connection, and orders by `<->` distance
then id. Both report `L2` and replace their collection on `prepare`.

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
| `qdrant` | Blocking HTTP adapter; plain L2, REST collection replace, batched upsert, exact query mode. |
| `pgvector` | Pooled synchronous Postgres adapter; plain L2, `COPY` load, sequential scan enforced. |

## Tests

Unit tests define a deterministic fake engine that checks idempotent prepare,
count mismatch, k above collection size, exact-mode failure and round-trip of
non-ASCII id strings. Integration tests cover kernel edge cases (zero, negative
zero, subnormal, absorption, overflow, ties) and a seeded random f64 interval
oracle. Set `KNN_AUDIT_INTEGRATION=1` to also run the Qdrant and pgvector
integration tests; they need the local engines on ports 6333 and 5432 and clean
up their pid-named collections and tables.
