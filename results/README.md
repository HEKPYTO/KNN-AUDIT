# Scorecards

Measured 2026-11-21 with `knn-audit 0.1.0` on Rust 1.100.0, arm64 workstation,
24 GiB. Dataset: Errpedia commit
`ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06`, `base.csv` sha256
`c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb`, 22,501
vectors x 128 `fp32`, 1,616 queries, top-10, exact rational ground truth.

Every engine ran in exact/brute-force mode. `--repeat N` performs N independent
prepares and full audits; answers are compared as id sets. `unstable` counts
cases whose set changed across reloads.

## Engine provenance

| Engine | Version | Exact mode | Configuration |
|---|---|---|---|
| reference | 0.1.0 | yes | `kernel=dyadic-rational`, `screening=certified-f64`, `tie_break=smaller-id` |
| Qdrant | 1.20.2 | yes | `search=exact`, `distance=euclid`, `hnsw_m=0`, `indexing_threshold=0`, `default_segment_number=1`, `indexed_vectors_count=0` |
| pgvector | 0.8.7 | yes | `plan=seq_scan`, `enable_indexscan=off`, `enable_bitmapscan=off`, `order=distance,id` |
| FAISS | 1.12.0 | yes | `index=IndexFlatL2`, `metric=squared_l2`, `overflow=reported_as_error` |

Every scorecard carries these configuration maps and the tool version, so a
reader does not have to trust this table.

## Scorecard

| Engine | Version | Wrong per reload | Wrong every reload | Unstable | Errors |
|---|---|---:|---:|---:|---:|
| reference | 0.1.0 | 0 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 771-828 | 449-452 | 848-852 | 0 |
| pgvector | 0.8.7 | 546 | 546 | 0 | 0 |
| FAISS | 1.12.0 | 545 | 545 | 0 | 31 |

Redacted evidence, with every failing case, per-reload counts, engine config,
and tool version:

| Artifact | Contents |
|---|---|
| [`scorecards/reference-0.1.0.json`](scorecards/reference-0.1.0.json) | control, 0 failures |
| [`scorecards/qdrant-1.20.2-repeat10-a.json`](scorecards/qdrant-1.20.2-repeat10-a.json) | Qdrant group A, 10 reloads |
| [`scorecards/qdrant-1.20.2-repeat10-b.json`](scorecards/qdrant-1.20.2-repeat10-b.json) | Qdrant group B, 10 reloads |
| [`scorecards/qdrant-1.20.2-repeat10-c.json`](scorecards/qdrant-1.20.2-repeat10-c.json) | Qdrant group C, 10 reloads |
| [`scorecards/qdrant-1.20.2-repeat10-d.json`](scorecards/qdrant-1.20.2-repeat10-d.json) | Qdrant group D, 10 reloads |
| [`scorecards/pgvector-0.8.7.json`](scorecards/pgvector-0.8.7.json) | pgvector, 3 reloads |
| [`scorecards/faiss-1.12.0.json`](scorecards/faiss-1.12.0.json) | FAISS, 3 reloads |

[`CHECKSUMS.txt`](CHECKSUMS.txt) pins every scorecard by sha256. Verify with
`shasum -a 256 -c results/CHECKSUMS.txt` from the repository root.

## Qdrant is a range, not a number

Qdrant does not contract tie order. Four committed groups of 10 independent
reloads plus one independent audit group were run from the same pins. Across
the 50 reloads the per-reload failure count was 771-828 (median 799, mean
801.5 over the 40 committed reloads), with missing 1229-1287, extra 1150-1208,
and rank swaps 2562-2638.

| Group | Wrong at least once | Wrong every reload | Unstable |
|---|---:|---:|---:|
| A | 1,147 | 452 | 848 |
| B | 1,148 | 449 | 852 |
| C | 1,148 | 452 | 851 |
| D | 1,148 | 449 | 851 |
| independent audit | 1,145 | 452 | 848 |

The groups agree within four cases on every aggregate. A single run is a
sample: use `--repeat 10` and report the observed range and per-reload counts.
The bands are observations, not statistical guarantees. The committed
scorecards carry all 10 runs each.

pgvector and FAISS were byte-identical across reloads.

## Failure categories

Qdrant columns are the observed range across groups A-D, widened where the
independent audit differed (neartie 189, order_disc 288). pgvector and FAISS
are stable, so their columns are also the per-reload counts.

| Category | Cases | Qdrant | pgvector | FAISS |
|---|---:|---:|---:|---:|
| absorb | 150 | 142-143 | 70 | 67 |
| collision_ladder | 83 | 83 | 83 | 83 |
| decimal_ingest | 12 | 12 | 0 | 0 |
| diff_overflow | 6 | 6 | 6 | 6 |
| exp_similarity | 8 | 0 | 0 | 0 |
| fp16_absorb | 6 | 0 | 0 | 0 |
| fp16_overflow | 9 | 0 | 0 | 0 |
| fp16_term | 14 | 0 | 0 | 0 |
| fp16_underflow | 96 | 96 | 0 | 0 |
| max_sentinel | 6 | 6 | 2 | 6 |
| neartie | 405 | 189-191 | 131 | 103 |
| normdot_cancel | 49 | 49 | 1 | 1 |
| normdot_overflow | 20 | 20 | 0 | 0 |
| order_disc | 480 | 288-289 | 111 | 137 |
| overflow_inf | 19 | 19 | 19 | 19 |
| sim_saturation | 30 | 30 | 0 | 0 |
| sq_round | 59 | 53 | 33 | 33 |
| sq_subnormal | 28 | 28 | 17 | 17 |
| sq_underflow | 50 | 50 | 21 | 21 |
| sqrt | 28 | 15 | 7 | 7 |
| sub | 58 | 58 | 45 | 45 |

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

Full method and counts: [`kernel-review.txt`](kernel-review.txt).

A separate engine review queried Qdrant's REST API and pgvector's SQL directly
for five failing and two passing cases per engine. All ten failing answers
reproduced byte-identically outside the adapter, and all four passing answers
matched ground truth. The sampled mismatches were exact ties at ranks 10 and 11
under the engine's own scores while exact arithmetic separated them. Details:
[`engine-review.txt`](engine-review.txt).

## Claim ladder

| Level | Claim allowed |
|---|---|
| Evidence | "Qdrant 1.20.2, pgvector 0.8.7, and FAISS 1.12.0, in exact mode, were run against Errpedia's 1,616 queries at top-10 on 2026-11-21." |
| Result | "They returned mathematically incorrect top-10 sets on 771-828, 546, and 545 queries per reload respectively; Qdrant was wrong on 1,145-1,148 distinct queries across each 10-reload group." |
| Inference | "Exact search in these versions is not numerically exact on this dataset." |
| Forbidden | "Vector databases are broken." / "Engine X is unusable." / "ANN recall was measured." |

Pair every public sentence with engine version, dataset commit, hardware,
exact-mode flag, and the scorecard link.

## Vendor disclosure

| Engine | Cases | Packet | Status |
|---|---:|---|---|
| Qdrant | 771-828 per reload, 449-452 always | `qdrant-1.20.2-repeat10-a.json`, `-b.json`, `-c.json`, `-d.json` | prepared, not sent |
| pgvector | 546 | `pgvector-0.8.7.json` | prepared, not sent |
| FAISS | 545, 31 overflow errors | `faiss-1.12.0.json` | prepared, not sent |

No vendor contact has been made from this repository. Sending a packet needs
owner action and dataset-license clarity; packets contain failing case ids,
expected ids, and actual ids, never query vectors.

## FAISS overflow

FAISS's f32 squared-L2 path cannot represent the distance on 31 queries: it
returns label `-1` (NaN internally) and no neighbors. The categories are
`overflow_inf` 19, `diff_overflow` 6, and `max_sentinel` 6. They are counted as
errors with all 10 expected ids missing, so the scorecard shows 31 errors and
1,454 missing on top of 1,144 extra.

## Dataset license and limits

The Errpedia repository declares no license (`license: null` on 2026-11-21).
This project does not redistribute the dataset: no `base.csv`, no collection,
and no query vectors are committed. Reports that contain query vectors stay
local and gitignored. CI fetches the pinned dataset commit at runtime. The
scorecards here contain only derived ids, counts, hashes, and engine
configuration.

Limits: exactness is tested, not ANN recall or latency. Qdrant tie order is
non-contractual. FAISS overflow queries return no neighbors. Engine behavior
can change between versions. Qdrant bands are observed samples, not
guarantees.

Release status: the scorecard is presented for owner acceptance; no tag has
been created.

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

The same steps run in the real-engine CI job. Qdrant uses `--repeat 10`;
reference, pgvector, and FAISS use `--repeat 3`. A fresh clone at `6ccc111` installed with
`cargo install --locked`, fetched the pinned dataset commit, passed the gated
test suite, and ran the script end to end; its Qdrant sample was 782-828 per
reload, 1,148 at least once, 449 every reload, 851 unstable. The workflow pins action SHAs, Rust 1.100.0, the CI image, both engine
image digests, the dataset commit, and the Python requirements. The workflow
has not yet executed on GitHub from this private workspace; the fresh-clone
run is the current execution evidence.
