# Errpedia Auditor

Prove whether an "exact" k-NN search is actually exact.

`knn-audit` runs the [Errpedia](https://arxiv.org/abs/2611.22474) numerical
conformance benchmark against real vector engines, compares every returned
top-10 to the dataset's exact rational ground truth, and emits a per-engine
scorecard with a reproducible failing case.

Independent work. Not affiliated with or endorsed by the benchmark authors.

## What it checks

The dataset stores finite `fp32` vectors and, for each query, the mathematically
correct top-10 under exact real arithmetic. The auditor asks an engine for its
top-10 in exact/brute-force mode and compares id sets:

| Finding | Meaning | CI failure |
|---|---|---|
| `missing` | correct id absent from the engine's answer | yes |
| `extra` | incorrect id present | yes |
| `rank_swap` | expected id at a different rank (reported; may co-occur with set differences) | no |
| `error` | engine failed the query | yes |

Ground truth is the oracle. The auditor never declares correctness from a
distance computed in floating point.

## Quick start

```sh
cargo build --release
git clone --depth 1 https://github.com/dynos01/errpedia .data/errpedia

./target/release/knn-audit dataset verify --dataset .data/errpedia --sample 100
./target/release/knn-audit run --dataset .data/errpedia --engine reference

docker run -d --name errpedia-qdrant -p 6333:6333 \
  qdrant/qdrant:v1.20-unprivileged@sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277
docker run -d --name errpedia-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 \
  pgvector/pgvector:0.8.7-pg19@sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5

./target/release/knn-audit run --dataset .data/errpedia --engine qdrant \
  --repeat 10 --out /tmp/qdrant.json --html /tmp/qdrant.html
./target/release/knn-audit run --dataset .data/errpedia --engine pgvector \
  --database-url postgres://postgres:postgres@127.0.0.1:5432/postgres \
  --out /tmp/pgvector.json --html /tmp/pgvector.html
./target/release/knn-audit run --dataset .data/errpedia --engine faiss \
  --faiss-python .venv-faiss/bin/python --out /tmp/faiss.json --html /tmp/faiss.html
./target/release/knn-audit report --input /tmp/qdrant.json --html /tmp/qdrant-report.html
```

`qdrant` defaults to `http://127.0.0.1:6333`; qdrant and pgvector default to the
`errpedia_audit` collection. Override with `--engine-url`, `--database-url`
and `--collection`. Use `--repeat N` for N independent prepare+audit runs; the
report adds per-run counts, `failed_all`, and `unstable` id-set changes.

FAISS needs Python 3.12 and `faiss-cpu` in a separate environment:

```sh
uv venv --python 3.12 .venv-faiss
uv pip install --python .venv-faiss/bin/python 'faiss-cpu==1.12.0'
```

Pass `--faiss-python .venv-faiss/bin/python`, or set `KNN_AUDIT_FAISS_PYTHON`;
the default interpreter is `python3`. FAISS uses `IndexFlatL2`, which is exact,
and reports squared L2 distances.

## Commands

```
knn-audit dataset verify --dataset <dir> [--sample N | --all] [--k 10]
knn-audit run --dataset <dir> --engine <reference|qdrant|pgvector|faiss> [options]
knn-audit report --input results/run.json --html results/run.html
knn-audit publish --input results/run.json --out results/scorecard.json
```

`--limit N` audits only the first N cases.

`run` writes `results/<engine>.json` (`knn-audit/v1`), prints a category table,
and exits `1` when `--fail-on-mismatch` is set and the answer has missing ids,
extra ids, or adapter errors. Rank swaps are reported in the table, categories
and report but do not fail the run. Every report carries the dataset sha256;
each failure carries the query vector, expected ids, and actual ids.

`publish` derives a `knn-audit-scorecard/v1` scorecard from a report. It drops
query vectors, dataset paths, and adapter configuration so the scorecard can be
shared without redistributing the dataset; categories and failures are sorted
for reproducible output.

## Results

Measured 2026-11-21 on 1,616 queries, 22,501 x 128 `fp32` vectors:

| Engine | Version | Wrong per reload | Wrong every reload | Unstable | Errors |
|---|---|---:|---:|---:|---:|
| reference | 0.1.0 | 0 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 771-825 | 449-452 | 848-852 | 0 |
| pgvector | 0.8.7 | 546 | 546 | 0 | 0 |
| FAISS | 1.12.0 | 545 | 545 | 0 | 31 |

Qdrant's tie order is non-contractual. Four independent groups of 10 reloads
were run; 771-825 queries were wrong per reload, 449-452 were wrong every time,
and 1,145-1,148 were wrong at least once per group. Its score is a range, not
one number. FAISS overflows f32 squared L2 on 31 queries and returns no
neighbors; those count as errors with all expected ids missing. See
[`results/README.md`](results/README.md) for per-run evidence, engine
configuration, category breakdown, the claim ladder, disclosure status, and
limits.

## Dataset

The benchmark data is not distributed with this repository and is not
redistributed by it. The dataset has no declared license. Point `--dataset` at
a local clone at commit `ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06`. The tool
verifies the `base.csv` sha256 and re-derives top-k with exact rational
arithmetic in `dataset verify`. Public scorecards contain no query vectors.

Set `ERRPEDIA_DATASET=<dir>` to point gated real-dataset tests at a local clone.

## CI

`ci` checks format, lint, unit tests and the release build. `integration` runs
the full audit against pinned Qdrant, pgvector and FAISS engines and publishes
redacted scorecards from `results/scorecards/`.

With both containers running and a local dataset clone, the audit runs locally
with `jq`, `curl` and `faiss-cpu` installed:

```sh
python3 -m pip install -r scripts/requirements-faiss.txt
cargo build --release
KNN_AUDIT=./target/release/knn-audit DATASET=.data/errpedia \
  FAISS_PYTHON=.venv-faiss/bin/python ./scripts/ci-audit.sh
```

## Crates

| Crate | Contents |
|---|---|
| `crates/knn-audit` | dataset loader, exact kernel, comparator, report, CLI |
| `crates/knn-audit-adapters` | engine adapters behind one `Engine` trait |

## License

Code: Apache-2.0. The Errpedia dataset remains the property of its authors.
