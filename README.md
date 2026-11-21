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
| `rank_swap` | same set, different order | no (reported) |
| `error` | engine failed the query | yes |

Ground truth is the oracle. The auditor never declares correctness from a
distance computed in floating point.

## Quick start

```sh
cargo build --release
git clone --depth 1 https://github.com/dynos01/errpedia .data/errpedia

./target/release/knn-audit dataset verify --dataset .data/errpedia --sample 100
./target/release/knn-audit run --dataset .data/errpedia --engine reference

./target/release/knn-audit run --dataset .data/errpedia --engine qdrant \
  --out /tmp/qdrant.json --html /tmp/qdrant.html
./target/release/knn-audit run --dataset .data/errpedia --engine pgvector \
  --database-url postgres://postgres:postgres@127.0.0.1:5432/postgres \
  --out /tmp/pgvector.json --html /tmp/pgvector.html
./target/release/knn-audit report --input /tmp/qdrant.json --html /tmp/qdrant-report.html
```

`qdrant` defaults to `http://127.0.0.1:6333`; both engines default to the
`errpedia_audit` collection. Override with `--engine-url`, `--database-url`
and `--collection`.

## Commands

```
knn-audit dataset verify --dataset <dir> [--sample N | --all] [--k 10]
knn-audit run --dataset <dir> --engine <reference|qdrant|pgvector> [options]
knn-audit report --input results/run.json --html results/run.html
```

`run` writes `results/<engine>.json` (`knn-audit/v1`), prints a category table,
and exits `1` when `--fail-on-mismatch` is set and the answer has missing ids,
extra ids, or adapter errors. Rank swaps are reported in the table, categories
and report but do not fail the run. Every failure carries the query vector,
expected ids, actual ids, and the dataset sha256 so the case can be reproduced
without the dataset copy that produced it.

## Dataset

The benchmark data is not distributed with this repository and is not
redistributed by it. Point `--dataset` at a local clone. The tool verifies the
`base.csv` sha256 and re-derives top-k with exact rational arithmetic in
`dataset verify`.

## Crates

| Crate | Contents |
|---|---|
| `crates/knn-audit` | dataset loader, exact kernel, comparator, report, CLI |
| `crates/knn-audit-adapters` | engine adapters behind one `Engine` trait |

## License

Code: Apache-2.0. The Errpedia dataset remains the property of its authors.
