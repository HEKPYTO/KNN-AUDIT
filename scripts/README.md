# Scripts

Run from the repository root.

## check-pins.sh

```sh
./scripts/check-pins.sh
```

Checks the dataset commit and `base.csv` sha256, both engine image digests,
Rust 1.100.0, Python 3.12, the 1,616 cases, 21 categories, top-10, the
`--repeat 10`/`--repeat 3` values and the pin-check step across the two
workflows, `ci-audit.sh` and both READMEs. It prints every missing pin and
exits 1 on any drift.

## ci-audit.sh

```sh
cargo build --release --locked
KNN_AUDIT=./target/release/knn-audit DATASET=.data/errpedia \
  FAISS_PYTHON=.venv-faiss/bin/python ./scripts/ci-audit.sh
```

Needs `jq`, `curl`, a local dataset clone at the pinned commit, pinned Qdrant
and pgvector services, and a Python 3.12 environment with `faiss-cpu==1.12.0`.

| Variable | Default | Purpose |
|---|---|---|
| `KNN_AUDIT` | `./target/release/knn-audit` | built binary |
| `DATASET` | `.data/errpedia` | dataset clone |
| `QDRANT_URL` | `http://127.0.0.1:6333` | Qdrant REST endpoint |
| `DATABASE_URL` | `postgres://postgres:postgres@127.0.0.1:5432/postgres` | pgvector endpoint |
| `FAISS_PYTHON` | `.venv-faiss/bin/python` | FAISS worker interpreter |
| `QDRANT_GROUP` | `a` | suffix for the published Qdrant scorecard |

The script verifies the dataset with `--expect-sha256`, runs a reference
repeat-1 control, then audits reference, Qdrant, pgvector and FAISS. Qdrant uses
`--repeat 10`; the others use `--repeat 3`. Raw reports live in a `mktemp -d`
directory removed on exit. Published scorecards go to `results/scorecards/`:
versioned names for reference, pgvector and FAISS, and
`qdrant-<version>-repeat10-<group>.json` for Qdrant. Set `QDRANT_GROUP` to `b`,
`c` or `d` to collect another committed sample.

Verify and engine output is echoed before every assertion; jq diagnostics are
left visible. Assertions cover report schema and shape, blocking-only
`summary.failed`, non-blocking `summary.order_only`, and scorecard redaction:
no `query`, `dataset.root` or `created_at`, and a present `tool.name` and
`tool.version`. Engine count assertions are smoke bands: CI is x86_64 while the
committed scorecards are arm64, and exact counts can shift with floating-point
rounding.

Exit `0` means the audit completed and every assertion passed — not that the
engines were exact. Findings do not fail the job; a drift, verification,
engine or assertion failure exits nonzero. `results/CHECKSUMS.txt` pins the
published scorecards; verify them with
`shasum -a 256 -c results/CHECKSUMS.txt` from the repository root.
