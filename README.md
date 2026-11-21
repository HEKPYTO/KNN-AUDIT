# KNN-AUDIT

Audit whether an "exact" k-NN search is actually exact.

Runs the pinned [Errpedia](https://arxiv.org/abs/2611.22474) benchmark
(22,501 fp32 vectors, 1,616 queries, top-10) against Qdrant, pgvector, FAISS,
and an exact reference. Every returned set is compared to exact rational
ground truth and published as a redacted scorecard. Independent work.

## Quick start

Prerequisites, pinned setup, and failure semantics: [`scripts/README.md`](scripts/README.md).

```sh
cargo build --release --locked
DATASET=.data/errpedia FAISS_PYTHON=.venv-faiss/bin/python ./scripts/ci-audit.sh
```

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

Qdrant tie order is non-contractual; CI is x86 and uses smoke bands. Full
evidence, claim ladder, limits, and checksums: [`results/README.md`](results/README.md).

Verify committed artifacts with `shasum -a 256 -c results/CHECKSUMS.txt`.

## Documentation

- [`crates/knn-audit/README.md`](crates/knn-audit/README.md) - CLI, dataset, report
- [`crates/knn-audit-adapters/README.md`](crates/knn-audit-adapters/README.md) - engine contract
- [`scripts/README.md`](scripts/README.md) - audit and pin scripts
- [`results/README.md`](results/README.md) - measured evidence

## Dataset and license

The benchmark is fetched at the pinned commit and is not redistributed. Pass
`--expect-sha256` to verify `base.csv`; public scorecards contain no query
vectors. Code is Apache-2.0; the Errpedia dataset remains the property of its
authors.
