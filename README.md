# KNN-AUDIT

Audit whether an "exact" k-NN search is actually exact.

`knn-audit` runs the [Errpedia](https://arxiv.org/abs/2611.22474) numerical
conformance benchmark against real vector engines, compares every returned
top-10 to exact rational ground truth, and emits a per-engine scorecard with a
reproducible failing case.

Independent work. Not affiliated with or endorsed by the benchmark authors.

## What it checks

The pinned dataset has 22,501 `fp32` vectors of dimension 128 and 1,616 queries
across 21 categories. Ground truth is the correct top-10 under exact real
arithmetic; the auditor never declares correctness from a floating-point
distance.

| Severity | Meaning | Blocking |
|---|---|---|
| `mismatch` | a correct id is missing or an incorrect id is present | yes |
| `error` | the engine failed the query | yes |
| `order_only` | ranks differ but the id set is correct | no |

`summary.failed` counts cases blocked by a mismatch or an error.
`summary.order_only` counts rank-only cases separately. `summary.unstable`
counts cases whose returned id set changed across reloads; a rank swap alone is
not instability. A successful CI job means the audit completed and its
assertions passed. It does not mean the engines were exact.

## Run the audit

Prerequisites: Rust 1.100.0, `jq`, `curl`, Python 3.12 with
`faiss-cpu==1.12.0`, and pinned Qdrant and pgvector containers. This is the
single canonical command block; crate READMEs link here instead of repeating
it.

```sh
cargo build --release --locked
git init -q .data/errpedia
git -C .data/errpedia remote add origin https://github.com/dynos01/errpedia
git -C .data/errpedia fetch --depth 1 -q origin ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06
git -C .data/errpedia checkout -q --detach FETCH_HEAD

docker run -d --name errpedia-qdrant -p 6333:6333 \
  qdrant/qdrant:v1.20-unprivileged@sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277
docker run -d --name errpedia-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 \
  pgvector/pgvector:0.8.7-pg19@sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5

uv venv --python 3.12 .venv-faiss
uv pip install --python .venv-faiss/bin/python -r scripts/requirements-faiss.txt

sha=c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb
./target/release/knn-audit dataset verify --dataset .data/errpedia --all --k 10 --expect-sha256 "$sha"
KNN_AUDIT=./target/release/knn-audit DATASET=.data/errpedia FAISS_PYTHON=.venv-faiss/bin/python ./scripts/ci-audit.sh
```

`shasum -a 256 -c results/CHECKSUMS.txt` verifies the committed artifacts, not a fresh sample.

`--repeat N` performs N independent prepare+audit passes; CI uses `--repeat 10`
for Qdrant and `--repeat 3` for reference, pgvector and FAISS. `--jobs N` caps
query workers (default: all logical CPUs). `--k` selects the top-k, pinned at
`10` here. `--fail-on-mismatch` makes the CLI exit 1 on blocking findings;
without it, findings are reported and the exit is 0.

`run` writes `knn-audit/v1` JSON, prints a category table, and keeps query
vectors locally. `publish` derives a byte-deterministic `knn-audit-scorecard/v1`
file: failures and categories are sorted and query vectors, dataset paths and
timestamps are removed. Every scorecard carries a `tool` object with `name` and
`version`.

## Results

Measured 2026-11-21 on the pinned dataset with exact/brute-force mode:

| Engine | Version | Blocking per reload | Blocking every reload | Order-only | Unstable | Errors |
|---|---|---:|---:|---:|---:|---:|
| reference | 0.1.0 | 0 | 0 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 543-603 | 200-204 | 219 | 851-852 | 0 |
| pgvector | 0.8.7 | 542 | 542 | 4 | 0 | 0 |
| FAISS | 1.12.0 | 541 | 541 | 4 | 0 | 31 |

Qdrant blocked 928-929 distinct queries in each of four committed 10-reload
groups; a GitHub CI sample from the same pins observed 889. Its tie order is
not contractual, so the score is a range over four committed groups; see
[`results/README.md`](results/README.md) for the claim ladder, disclosure
status, limits and artifact links.

## Dataset

The benchmark data is not distributed with this repository. Point `--dataset`
at a clone of commit `ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06`; pass
`--expect-sha256` to verify `base.csv` against
`c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb`. The dataset
declares no license, and public scorecards contain no query vectors. Set
`ERRPEDIA_DATASET=<dir>` for gated real-dataset tests.

## CI

`ci` checks pins, format, lint, unit tests and the release build. `integration`
checks pins, runs the full audit against the pinned engines, and uploads
`results/scorecards/`. CI job success means the audit completed; it is not a
correctness verdict. See [`scripts/README.md`](scripts/README.md) for the
script interface and exit semantics.

## Crates

| Crate | Contents |
|---|---|
| `crates/knn-audit` | dataset loader, comparator, report, CLI |
| `crates/knn-audit-adapters` | exact kernel and engine adapters behind one `Engine` trait |

## License

Code: Apache-2.0. The Errpedia dataset remains the property of its authors.
