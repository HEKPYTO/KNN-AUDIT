# Scorecards

Measured 2026-11-21 against the real Errpedia dataset: 22,501 vectors x 128
`fp32`, 1,616 queries, top-10, exact rational ground truth. Dataset commit
`ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06`; `base.csv` sha256
`c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb`.

Every run used exact/brute-force mode: Qdrant `params.exact=true` with no
index, pgvector with index and bitmap scans disabled, FAISS `IndexFlatL2`.
`--repeat N` performs N independent prepares and full audits; answers are
compared as id sets. `unstable` counts cases whose set changed across reloads.

## Scorecard

| Engine | Version | Wrong per reload | Wrong every reload | Unstable | Errors |
|---|---|---:|---:|---:|---:|
| reference | 0.1.0 | 0 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 782-816 | 450 | 851 | 0 |
| pgvector | 0.8.7 | 546 | 546 | 0 | 0 |
| FAISS | 1.12.0 | 545 | 545 | 0 | 31 |

Full redacted evidence, with every failing case and per-reload counts:

- [`scorecards/reference-0.1.0.json`](scorecards/reference-0.1.0.json)
- [`scorecards/qdrant-1.20.2-repeat10.json`](scorecards/qdrant-1.20.2-repeat10.json)
- [`scorecards/pgvector-0.8.7.json`](scorecards/pgvector-0.8.7.json)
- [`scorecards/faiss-1.12.0.json`](scorecards/faiss-1.12.0.json)

## Qdrant is a range, not a number

Qdrant does not guarantee tie order. Across 10 independently prepared
collections the per-reload failure count was 782-816 (missing 1223-1294,
extra 1144-1215, rank swaps 2548-2629). Over those 10 reloads:

- 1,147 queries were wrong at least once.
- 450 queries were wrong in every reload.
- 851 queries changed answer between reloads.

A single run is a sample. Use `--repeat 10` and report the range and the
per-reload counts. The committed scorecard carries all 10 runs.

pgvector and FAISS were byte-identical across reloads.

## FAISS overflow

FAISS's f32 squared-L2 path overflows to NaN on 31 queries: `overflow_inf` 19,
`diff_overflow` 6, `max_sentinel` 6. FAISS returns no neighbors for those.
They are counted as errors with all 10 expected ids missing, so the scorecard
shows 31 errors and 1,454 missing on top of 1,144 extra.

## Kernel verification

An independent reviewer re-derived the kernel from scratch with a Python
`fractions.Fraction` oracle, independent binary32 rounding, and integer-scaled
dyadic arithmetic. It checked 5,120 randomized cases, 1,600 adversarial cases,
and 657 real absorb/near-tie/overflow/collision cases (7,377 total):

- `dataset verify --all`: 0 mismatches.
- Reference engine `--repeat 3`: 0 failures, 0 errors, 0 rank swaps.
- Exact ties: smaller id first, every time.
- Worst f64 screening error 8.61e-16 against a 4.55e-13 bound, about 529x
  headroom.

## Claim ladder

| Level | Claim allowed |
|---|---|
| Evidence | "Qdrant 1.20.2, pgvector 0.8.7, and FAISS 1.12.0, in exact mode, were run against Errpedia's 1,616 queries at top-10 on 2026-11-21." |
| Result | "They returned mathematically incorrect top-10 sets on 782-816, 546, and 545 queries per reload respectively." |
| Inference | "Exact search in these versions is not numerically exact on this dataset." |
| Forbidden | "Vector databases are broken." / "Engine X is unusable." / "ANN recall was measured." |

Pair every public sentence with engine version, dataset commit, hardware,
exact-mode flag, and the scorecard link.

## Vendor disclosure

| Engine | Cases | Packet | Status |
|---|---:|---|---|
| Qdrant | 782-816 per reload, 450 always | `qdrant-1.20.2-repeat10.json` | prepared, not sent |
| pgvector | 546 | `pgvector-0.8.7.json` | prepared, not sent |
| FAISS | 545, 31 overflow errors | `faiss-1.12.0.json` | prepared, not sent |

No vendor contact has been made from this repository. Sending a packet needs
owner approval and dataset-license clarity; packets contain failing case ids,
expected ids, and actual ids, never query vectors.

## Dataset license and limits

The Errpedia repository declares no license (`license: null` on 2026-11-21).
This project does not redistribute the dataset: no `base.csv`, no collection,
and no query vectors are committed. Reports that contain query vectors stay
local and gitignored. CI fetches the pinned dataset commit at runtime. The
scorecards here contain only derived ids, counts, and hashes.

Limits: exactness is tested, not ANN recall or latency. Qdrant tie order is
non-contractual. FAISS overflow queries return no neighbors. Engine behavior
can change between versions.

## Reproduce

```sh
docker run -d --name errpedia-qdrant -p 6333:6333 \
  qdrant/qdrant:v1.20-unprivileged@sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277
docker run -d --name errpedia-pg -e POSTGRES_PASSWORD=postgres -p 5432:5432 \
  pgvector/pgvector:0.8.7-pg19@sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5

git init -q .data/errpedia
git -C .data/errpedia remote add origin https://github.com/dynos01/errpedia
git -C .data/errpedia fetch --depth 1 -q origin ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06
git -C .data/errpedia checkout -q --detach FETCH_HEAD

cargo build --release --locked
KNN_AUDIT=./target/release/knn-audit FAISS_PYTHON=.venv-faiss/bin/python \
  DATASET=.data/errpedia ./scripts/ci-audit.sh
```

The same steps run in the real-engine CI job. The Qdrant range artifact uses
`--repeat 10`; the CI job uses `--repeat 3` for time.
