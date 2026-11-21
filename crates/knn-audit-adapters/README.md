# knn-audit-adapters
Engine contract and the in-process exact control. This crate never depends on
`knn-audit`.

`qdrant` is a blocking REST adapter using Qdrant's exact search with plain L2.
It pins one flat segment (`hnsw_config.m=0`, `indexing_threshold=0`,
`default_segment_number=1`) but Qdrant does not guarantee tie order, so top-k
sets can still change across reloads.
`pgvector` is a synchronous Postgres adapter that loads vectors with `COPY`,
disables index scans on every pooled connection, and orders by `<->` distance
then id. Both report `L2` and replace their collection on `prepare`.
`faiss` drives `IndexFlatL2` through a JSON-lines Python worker embedded in the
binary with `include_str!`. The index is exact and returns squared L2 distances.
Set up the interpreter with
`uv venv --python 3.12 .venv-faiss` and
`uv pip install --python .venv-faiss/bin/python 'faiss-cpu==1.12.0'`; the engine
uses `KNN_AUDIT_FAISS_PYTHON` or `--faiss-python` and defaults to `python3`.

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
| `faiss` | Embedded JSON-lines Python worker over exact `IndexFlatL2`; squared L2, internal-index to dataset-id mapping. |

## Tests

Unit tests define a deterministic fake engine that checks idempotent prepare,
count mismatch, k above collection size, exact-mode failure and round-trip of
non-ASCII id strings. Integration tests cover kernel edge cases (zero, negative
zero, subnormal, absorption, overflow, ties) and a seeded random f64 interval
oracle. Set `KNN_AUDIT_INTEGRATION=1` to also run the Qdrant, pgvector and FAISS
integration tests; Qdrant and pgvector need the local engines on ports 6333 and
5432, and FAISS needs a Python 3.12 interpreter with `faiss-cpu==1.12.0` in
`KNN_AUDIT_FAISS_PYTHON`. Each test prints a skip note when its engine is not
available. The CLI test suite also drives the FAISS adapter with a fake Python
worker that needs no FAISS install.
