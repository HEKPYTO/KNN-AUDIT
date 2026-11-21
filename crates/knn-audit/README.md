# knn-audit

Library and `knn-audit` binary that compare an engine's top-k against the
dataset's shipped ground truth. Canonical commands live in the
[root README](../../README.md).

## Modules

| Module | Responsibility |
|---|---|
| `dataset` | Loads `data/base.csv` and collection files, validates shape and ids, checks the pinned sha256, recomputes sampled ground truth. |
| `exact` | Re-exports the dyadic-rational kernel from `knn-audit-adapters`. |
| `compare` | Classifies a query as `clean`, `order_only`, `mismatch` or `error` and records rank swaps and a diagnostic distance gap. |
| `report` | Serializes `knn-audit/v1` reports and `knn-audit-scorecard/v1` scorecards, and renders escaped HTML. |
| `run` | Prepares the engine, executes cases in parallel, and aggregates severity, stability and category counts. |
| `cli` | Parses commands, chooses exit codes, and writes outputs. |

## Semantics

Defaults: `--sample 100`, `--k 10`, `--repeat 1`, `--out results/<engine>.json`,
and all logical CPUs for `--jobs`. `dataset verify` checks the pinned `base.csv`
sha256 when the collection has the shipped benchmark's shape; pass
`--expect-sha256` to require it explicitly. A set-level tie at the k boundary
is reported as missing/extra ids, never hidden by choosing a preferred id.

`--repeat N` performs N independent prepare+audit passes. `summary.failed`
counts cases blocked in at least one pass by a mismatch or an error;
`failed_all` counts cases blocked in every pass. `summary.order_only` counts
cases with rank swaps but a correct id set; they never block and never count as
unstable. `unstable` counts cases whose id set changed across passes. `runs[]`
holds per-pass counts; a failure carries `failed_repetitions` and
`unstable_actual_ids`. With `--repeat 1`, stability data is empty.

`publish` removes query vectors, dataset paths and timestamps, sorts failures
and categories, and keeps `tool.name`, `tool.version` and engine configuration.
The result is byte-deterministic and safe to share.

Exit codes: `0` clean or findings reported, `1` with `--fail-on-mismatch` when
`summary.failed > 0`, `2` usage, dataset, adapter or report errors. Order-only
findings never fail a run.

## Tests

`tests/` exercises the public seams with checked-in fixtures, including
severity, repeat aggregation and redaction, and, when `.data/errpedia` is
present, a gated real-dataset verification.
