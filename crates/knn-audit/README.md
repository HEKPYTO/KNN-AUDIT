# knn-audit

Library and `knn-audit` binary that compare an engine's top-k against the
dataset's shipped ground truth.

## Modules

| Module | Responsibility |
|---|---|
| `dataset` | Loads `data/base.csv` and `data/<group>/<case>.json`, validates dimensions, finite values and id ranges, hashes the collection, and recomputes sampled ground truth. |
| `exact` | Re-exports the dyadic-rational kernel from `knn-audit-adapters`. |
| `compare` | Reports missing ids, extra ids, rank swaps and an f64 distance gap used only as a diagnostic. |
| `report` | Serializes `knn-audit/v1` JSON and renders one escaped, self-contained HTML page. |
| `run` | Prepares the engine, executes cases in parallel, and builds the report. |
| `cli` | Parses commands, chooses exit codes, and writes outputs. |

## Commands

```sh
cargo run -p knn-audit -- dataset verify --dataset ../../.data/errpedia --sample 100
cargo run -p knn-audit -- dataset verify --dataset ../../.data/errpedia --all
cargo run -p knn-audit -- run --dataset ../../.data/errpedia --engine reference \
  --html results/reference.html --fail-on-mismatch
cargo run -p knn-audit -- run --dataset ../../.data/errpedia --engine qdrant \
  --repeat 3 --out results/qdrant.json
cargo run -p knn-audit -- report --input results/reference.json --html results/reference.html

Defaults: `--sample 100`, `--k 10`, `--repeat 1`, `--out results/<engine>.json`,
and all logical CPUs for `--jobs`. `dataset verify` checks the pinned `base.csv`
sha256 when the loaded collection has the shipped benchmark's shape. A set-level
tie at the k boundary is reported as missing/extra ids; it is never hidden by
choosing a preferred id.

`--repeat N` runs N independent audits: each one calls `prepare` again and
runs all cases. `summary.failed` counts cases failing any run, `failed_all`
counts cases failing every run, and `unstable` counts cases with distinct id
sets across runs (order alone is not instability). `runs[]` holds each run's
counts; each failure carries `failed_repetitions` and `unstable_actual_ids`.
With `N=1` counts match a single audit and stability data is empty.

Exit codes: `0` clean, `1` with `--fail-on-mismatch` only for missing ids, extra
ids or adapter errors, `2` usage, dataset, adapter or report errors. Rank swaps
are reported but never fail the run.

## Tests

`tests/` exercises the public seams with the checked-in fixtures and, when
`.data/errpedia` is present, a gated real-dataset verification.
