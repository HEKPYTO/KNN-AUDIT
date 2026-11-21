# KNN-AUDIT

[![ci](https://github.com/HEKPYTO/KNN-AUDIT/actions/workflows/ci.yml/badge.svg)](https://github.com/HEKPYTO/KNN-AUDIT/actions/workflows/ci.yml)
[![integration](https://github.com/HEKPYTO/KNN-AUDIT/actions/workflows/integration.yml/badge.svg)](https://github.com/HEKPYTO/KNN-AUDIT/actions/workflows/integration.yml)
[![release](https://img.shields.io/github/v/release/HEKPYTO/KNN-AUDIT)](https://github.com/HEKPYTO/KNN-AUDIT/releases)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

Audit whether an "exact" k-NN search is actually exact.

Runs the pinned [Errpedia](https://arxiv.org/abs/2611.22474) benchmark
(22,501 fp32 vectors, 1,616 queries, top-10) against Qdrant, pgvector, FAISS,
and an exact reference. Every returned set is compared to exact rational
ground truth and published as a redacted scorecard. Independent work.

## Features

- One severity model: missing/extra ids and engine errors block; rank-only
  swaps are reported separately.
- Deterministic scorecards with engine config, dataset hash, per-reload
  counts, and failing cases.
- Reference, Qdrant, pgvector, and FAISS adapters.
- Pinned dataset, engine images, Rust/Python versions, and CI actions.

## Quick start

Requirements: Rust 1.100.0, Docker, `jq`, `curl`, Python 3.12.

```sh
# Dataset, pinned by commit; ci-audit.sh verifies base.csv sha256.
git init -q .data/errpedia
git -C .data/errpedia remote add origin https://github.com/dynos01/errpedia
git -C .data/errpedia fetch --depth 1 -q origin ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06
git -C .data/errpedia checkout -q --detach FETCH_HEAD

# Exact-mode engines.
docker run -d --name errpedia-qdrant -p 6333:6333 \
  qdrant/qdrant:v1.20-unprivileged@sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277
docker run -d --name errpedia-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 \
  pgvector/pgvector:0.8.7-pg19@sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5

# FAISS worker environment.
python3.12 -m venv .venv-faiss
.venv-faiss/bin/python -m pip install -r scripts/requirements-faiss.txt

# Build and audit.
cargo build --release --locked
DATASET=.data/errpedia FAISS_PYTHON=.venv-faiss/bin/python ./scripts/ci-audit.sh
```

`ci-audit.sh` checks the dataset hash, runs all four engines, and writes
`results/scorecards/`. Set `QDRANT_GROUP=b`, `c` or `d` for another Qdrant
sample.

`failed` means a missing/extra id or an engine error. `order_only` means the
id set is correct but ranks differ; it never blocks. `--fail-on-mismatch`
exits 1 on blocking findings. CI success means the audit completed, not that
the engines were exact.

## Results

arm64 measurement, exact/brute-force mode:

| Engine | Version | Blocking per reload | Order-only | Errors |
|---|---|---:|---:|---:|
| reference | 0.1.0 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 543-603 | 219 | 0 |
| pgvector | 0.8.7 | 542 | 4 | 0 |
| FAISS | 1.12.0 | 541 | 4 | 31 |

Qdrant tie order is non-contractual. Committed evidence, claim ladder, limits,
checksums and the x86 CI caveat: [`results/README.md`](results/README.md).

## Project layout

| Path | Contents |
|---|---|
| `crates/knn-audit` | dataset loader, exact comparator, report, CLI |
| `crates/knn-audit-adapters` | exact kernel and engine adapters |
| `scripts` | audit runner, pin check, FAISS requirements |
| `results` | scorecards, checksums, reviews |
| `fixtures` | small datasets for tests |

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
ERRPEDIA_DATASET=.data/errpedia cargo test --workspace --locked
```

More detail:

- [`crates/knn-audit/README.md`](crates/knn-audit/README.md) - CLI, dataset, report
- [`crates/knn-audit-adapters/README.md`](crates/knn-audit-adapters/README.md) - engine contract
- [`scripts/README.md`](scripts/README.md) - audit and pin scripts
- [`results/README.md`](results/README.md) - measured evidence

## Dataset and license

The benchmark is fetched at the pinned commit and is not redistributed; public
scorecards contain no query vectors. Code is Apache-2.0; the Errpedia dataset
remains the property of its authors.
