# Scorecards

Measured 2026-11-21 on the real Errpedia dataset: 22,501 vectors x 128 `fp32`,
1,616 queries, top-10, exact rational ground truth. `base.csv` sha256
`c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb`.

| Engine | Version | Wrong cases (any) | Wrong in every run | Unstable | Errors |
|---|---|---:|---:|---:|---:|
| reference | 0.1.0 | 0/1616 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 1056/1616 | 533 | 676 | 0 |
| pgvector | 0.8.7 | 546/1616 | 546 | 0 | 0 |
| FAISS | 1.12.0 | 545/1616 | 545 | 0 | 31 |

All engine runs used exact/brute-force search with `--repeat 3`.

- Qdrant's tie order is non-contractual. Independent reloads pick different ids
  among equal scores, so its score is a range: observed 1051-1058 wrong any,
  526-533 wrong in every run, 676-684 unstable across reload groups. The report
  emits per-run counts; do not read a single run as the whole result.
- pgvector is deterministic across reloads: 1230 missing, 1230 extra, 547 rank
  swaps every run.
- FAISS is deterministic; 31 queries overflow f32 squared L2 to NaN, FAISS
  returns no neighbors, and those cases count as errors with all 10 expected ids
  missing: 1454 missing, 1144 extra, 547 rank swaps.
- The in-process reference engine matches the shipped ground truth on all
  1,616 queries, which validates the exact kernel.

Reproduce:

```sh
DATA=.data/errpedia
cargo build --release
./target/release/knn-audit dataset verify --dataset "$DATA" --all
./target/release/knn-audit run --dataset "$DATA" --engine reference --repeat 3
./target/release/knn-audit run --dataset "$DATA" --engine qdrant --repeat 3
./target/release/knn-audit run --dataset "$DATA" --engine pgvector --repeat 3
./target/release/knn-audit run --dataset "$DATA" --engine faiss \
  --faiss-python .venv-faiss/bin/python --repeat 3
```

Reports are generated locally and are not committed: they contain failing query
vectors, which are dataset content and are not redistributed here.
