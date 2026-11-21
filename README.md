# KNN-AUDIT

Audit whether an "exact" k-NN search is actually exact.

Runs the pinned [Errpedia](https://arxiv.org/abs/2611.22474) benchmark
(22,501 fp32 vectors, 1,616 queries, top-10) against Qdrant, pgvector, FAISS,
and an exact reference. Every returned set is compared to exact rational
ground truth and published as a redacted scorecard. Independent work.

## Requirements

Rust 1.100.0, Docker, `jq`, `curl`, and Python 3.12. All Rust commands use
`--locked`.

## Run from scratch

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

`ci-audit.sh` checks the dataset hash, runs reference, Qdrant, pgvector and
FAISS, and writes `results/scorecards/`. Export `QDRANT_GROUP=b`, `c` or `d`
for another Qdrant sample.

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

## Documentation

- [`crates/knn-audit/README.md`](crates/knn-audit/README.md) - CLI, dataset, report
- [`crates/knn-audit-adapters/README.md`](crates/knn-audit-adapters/README.md) - engine contract
- [`scripts/README.md`](scripts/README.md) - audit and pin scripts
- [`results/README.md`](results/README.md) - measured evidence

## Dataset and license

The benchmark is fetched at the pinned commit and is not redistributed; public
scorecards contain no query vectors. Code is Apache-2.0; the Errpedia dataset
remains the property of its authors.
