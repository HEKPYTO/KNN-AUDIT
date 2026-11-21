# knn-audit-adapters

Engine contract and the in-process exact control. This crate never depends on
`knn-audit`. Canonical commands live in the [root README](../../README.md).

`qdrant` is a blocking REST adapter using exact search with plain L2. It pins
one flat segment (`hnsw_config.m=0`, `indexing_threshold=0`,
`default_segment_number=1`), but Qdrant does not guarantee tie order, so top-k
sets can change across reloads. `pgvector` loads vectors with `COPY`, disables
index scans, and orders by distance then id. Both replace their collection on
`prepare` and expose `cleanup()` to drop it. `faiss` drives `IndexFlatL2` through
an embedded Python worker and reports squared L2.

## Contract

`Engine` has `name`, `version`, `config`, `prepare`, `search` and
`capabilities`. There is no `reset`: `prepare` replaces the collection and is
idempotent, and a count mismatch is an `AdapterError`. Qdrant and pgvector add
`cleanup()` for explicit teardown; the FAISS worker is killed by `Drop`.
`search` returns `Hit { id, distance }` in engine order, never re-sorting ties.
`CollectionView` carries numeric ids and a flat `f32` vector buffer. `Caps`
declares `exact_mode` and a `Metric` (`SquaredL2` or `L2`).

Collection and table names must start with an ASCII letter or `_`, contain only
ASCII alphanumerics or `_`, and be at most 63 characters. Anything else is an
adapter error before any network call.

## Modules

| Module | Responsibility |
|---|---|
| `exact` | Certified f64 screening at `8γ(4·dim)` relative error, dyadic-rational `BigInt` comparison for near-boundary candidates, and exact top-k with smaller-id tie-break. |
| `reference` | In-process engine backed by the exact kernel; squared L2, exact mode, no f64-derived ordering decision. |
| `qdrant` | Blocking HTTP adapter; exact search, REST collection replace, batched upsert, flat segment config, `cleanup()`. |
| `pgvector` | Pooled synchronous Postgres adapter; rejects ids above `i64::MAX` before `COPY`, sequential scan enforced, `cleanup()`. |
| `faiss` | Embedded JSON-lines Python worker over exact `IndexFlatL2`; squared L2, internal-index to dataset-id mapping. |

The FAISS worker is serial behind a mutex: each request is a JSON object with
an `op` (`version`, `create`, `add`, `count`, `search`) and one JSON response
line per request; failures return an `error` field. Requests time out after
120 seconds and terminate the worker. Index overflow is an explicit adapter
error. The interpreter comes from `KNN_AUDIT_FAISS_PYTHON` or `--faiss-python`
and defaults to `python3`; use Python 3.12 with `faiss-cpu==1.12.0`.

## Tests

Unit tests define a deterministic fake engine that checks idempotent prepare,
count mismatch, k above collection size, exact-mode failure and non-ASCII ids.
Integration tests cover kernel edge cases (zero, negative zero, subnormal,
absorption, overflow, ties) and a seeded random f64 interval oracle. Set
`KNN_AUDIT_INTEGRATION=1` to also run the Qdrant, pgvector and FAISS tests;
they need local engines on ports 6333 and 5432 and a Python 3.12 interpreter
with `faiss-cpu==1.12.0`. Each test prints a skip note when its engine is not
available. The CLI suite drives FAISS with a fake Python worker that needs no
FAISS install.
