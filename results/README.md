# Scorecards

Measured 2026-11-21 with `knn-audit 0.1.0` on Rust 1.100.0, arm64 workstation,
24 GiB. Dataset: Errpedia commit
`ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06`, `base.csv` sha256
`c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb`, 22,501
vectors x 128 `fp32`, 1,616 queries across 21 categories, top-10, exact
rational ground truth. FAISS ran under Python 3.12 with `faiss-cpu==1.12.0`.

Every engine ran in exact/brute-force mode. `failed` counts queries blocked by
a missing or extra id, or by an engine error. `order_only` counts rank swaps
with a correct id set: they are reported but never block. `unstable` counts
cases whose returned id set changed across reloads. Engine configuration and
tool provenance are inside each scorecard.

## Headline

| Engine | Version | Blocking per reload | Blocking every reload | Order-only | Unstable | Errors |
|---|---|---:|---:|---:|---:|---:|
| reference | 0.1.0 | 0 | 0 | 0 | 0 | 0 |
| Qdrant | 1.20.2 | 543-603 | 200-204 | 219 | 851-852 | 0 |
| pgvector | 0.8.7 | 542 | 542 | 4 | 0 | 0 |
| FAISS | 1.12.0 | 541 | 541 | 4 | 0 | 31 |

Each stable engine was byte-identical across its three reloads. FAISS's 31
errors are f32 squared-L2 overflow queries with no neighbors returned; they are
blocking errors, while its 4 order-only cases are rank swaps on otherwise
correct id sets.

## Qdrant is a range, not a number

Four committed groups of `--repeat 10` independent reloads were run from the
same pins. Per-reload counts come from `runs[]`: blocking failures 543-603,
missing ids 1,226-1,306, extra ids 1,147-1,227 and rank swaps 2,547-2,654.

The `summary` counts are distinct cases across each 10-reload group: 928-929
blocked, missing ids 1,606-1,617, extra ids 1,527-1,538 and rank swaps
2,573-2,601. Of these, 200-204 were blocked in every reload, 851-852 changed id
sets, and a further 219 per group were order-only (correct id set in every
reload, ranks moved only); order-only cases are not part of `failed`.

| Group | Blocked at least once | Blocked every reload | Order-only | Unstable |
|---|---:|---:|---:|---:|
| A | 929 | 202 | 219 | 851 |
| B | 929 | 201 | 219 | 852 |
| C | 929 | 200 | 219 | 852 |
| D | 928 | 204 | 219 | 851 |

One GitHub-hosted CI sample from the same pins observed 889 distinct blocked
and 812 unstable, with 204 blocked every reload and 219 order-only. A single
10-reload sample can sit below the committed groups; use the committed
scorecards for the pinned evidence, not one run.

Qdrant does not contract tie order. One run is a sample; use `--repeat 10` and
report the observed range. These bands are observations, not statistical
guarantees.

## Artifacts

Redacted scorecards carry every failing case, per-reload counts, engine config
and tool version; query vectors, dataset paths and timestamps are removed.

| Artifact | Contents |
|---|---|
| [`scorecards/reference-0.1.0.json`](scorecards/reference-0.1.0.json) | control, 3 reloads, 0 findings |
| [`scorecards/qdrant-1.20.2-repeat10-a.json`](scorecards/qdrant-1.20.2-repeat10-a.json) | Qdrant group A, 10 reloads |
| [`scorecards/qdrant-1.20.2-repeat10-b.json`](scorecards/qdrant-1.20.2-repeat10-b.json) | Qdrant group B, 10 reloads |
| [`scorecards/qdrant-1.20.2-repeat10-c.json`](scorecards/qdrant-1.20.2-repeat10-c.json) | Qdrant group C, 10 reloads |
| [`scorecards/qdrant-1.20.2-repeat10-d.json`](scorecards/qdrant-1.20.2-repeat10-d.json) | Qdrant group D, 10 reloads |
| [`scorecards/pgvector-0.8.7.json`](scorecards/pgvector-0.8.7.json) | pgvector, 3 reloads |
| [`scorecards/faiss-1.12.0.json`](scorecards/faiss-1.12.0.json) | FAISS, 3 reloads |

[`CHECKSUMS.txt`](CHECKSUMS.txt) pins every scorecard by sha256. Verify from the
repository root with `shasum -a 256 -c results/CHECKSUMS.txt`. Review notes:
[`kernel-review.txt`](kernel-review.txt),
[`engine-review.txt`](engine-review.txt).

## Claim ladder

| Level | Claim allowed |
|---|---|
| Evidence | "Qdrant 1.20.2, pgvector 0.8.7 and FAISS 1.12.0 in exact mode were run against the pinned Errpedia 1,616-query top-10 benchmark on 2026-11-21." |
| Result | "Blocking failures occurred on 543-603 Qdrant queries per reload (928-929 distinct across the four committed 10-reload groups), 542 pgvector queries and 541 FAISS queries (510 id-set mismatches, 31 overflow errors). Order-only rank swaps were 219, 4 and 4 cases and did not block." |
| Inference | "Exact search in these versions is not numerically exact on this dataset." |
| Forbidden | "ANN recall was measured", "all vector databases are broken", "engine X is unusable", "CI passed so the engines are exact", or treating a rank swap as a set failure. |

Pair every public sentence with engine version, dataset commit, hardware,
exact-mode flag and a scorecard link. CI job success means the audit completed;
it is not a correctness verdict.

## Vendor disclosure

| Engine | Blocking cases | Packet | Status |
|---|---|---|---|
| Qdrant | 543-603 per reload, 200-204 always, 928-929 distinct per group | four `qdrant-1.20.2-repeat10-*.json` scorecards | prepared, not sent |
| pgvector | 542 | `pgvector-0.8.7.json` | prepared, not sent |
| FAISS | 541 blocked (510 id-set mismatches, 31 overflow errors) | `faiss-1.12.0.json` | prepared, not sent |

No vendor contact has been made from this repository. Sending a packet needs
owner action and dataset-license clarity; packets contain failing case ids,
expected ids and actual ids, never query vectors.

## Limits

Exactness is tested, not ANN recall or latency. Qdrant tie order is
non-contractual, so its numbers are observed samples. FAISS overflow queries
return no neighbors. Engine behavior can change between versions, and these
results do not transfer to other datasets or configurations. The dataset
declares no license; it is fetched at runtime and never redistributed. No tag
has been created; the scorecards await owner acceptance.

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
shasum -a 256 -c results/CHECKSUMS.txt
```

The FAISS interpreter needs Python 3.12 and `faiss-cpu==1.12.0`. The script
pins `--repeat 10` for Qdrant and `--repeat 3` for reference, pgvector and
FAISS, and `QDRANT_GROUP=b|c|d` publishes the other committed groups. The
workflow has not yet executed on GitHub from this private workspace; the
committed scorecards are the execution evidence.
