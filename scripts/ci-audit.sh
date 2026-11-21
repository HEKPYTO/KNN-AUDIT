#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

KNN_AUDIT=${KNN_AUDIT:-./target/release/knn-audit}
DATASET=${DATASET:-.data/errpedia}
QDRANT_URL=${QDRANT_URL:-http://127.0.0.1:6333}
DATABASE_URL=${DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/postgres}
FAISS_PYTHON=${FAISS_PYTHON:-.venv-faiss/bin/python}
QDRANT_GROUP=${QDRANT_GROUP:-a}
if [[ ! $QDRANT_GROUP =~ ^[abcd]$ ]]; then
  echo "QDRANT_GROUP must be a, b, c or d" >&2
  exit 2
fi
EXPECT_SHA256=c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb
# Dataset commit ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06.

# Dataset shape and repeats are pinned: 1,616 cases, 21 categories, top-10;
# Qdrant --repeat 10, reference/pgvector/FAISS --repeat 3.
tmp=$(mktemp -d "${TMPDIR:-/tmp}/knn-audit.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
scorecards=results/scorecards
mkdir -p "$scorecards"

# The pinned Qdrant image has no health tool; poll its REST root instead.
for _ in $(seq 1 60); do
  if curl -fsS "$QDRANT_URL/" >/dev/null; then
    break
  fi
  sleep 1
done
curl -fsS "$QDRANT_URL/"

verify_status=0
verify=$("$KNN_AUDIT" dataset verify --dataset "$DATASET" --all --k 10 \
  --expect-sha256 "$EXPECT_SHA256" 2>&1) || verify_status=$?
echo "$verify"
[[ ${verify_status:-0} -eq 0 ]]
[[ "$verify" == "dataset verify: 0/1616 cases mismatch"* ]]

control_status=0
control=$("$KNN_AUDIT" run --dataset "$DATASET" --engine reference --repeat 1 --k 10 \
  --expect-sha256 "$EXPECT_SHA256" --out "$tmp/reference-repeat1.json" 2>&1) || control_status=$?
echo "$control"
[[ ${control_status:-0} -eq 0 ]]
jq -e --arg sha "$EXPECT_SHA256" '
  .schema == "knn-audit/v1" and .dataset.base_sha256 == $sha
  and .summary.cases == 1616 and .summary.failed == 0 and .summary.order_only == 0
  and (.runs | length) == 1 and (.failures | length) == 0
' "$tmp/reference-repeat1.json"

for engine in reference qdrant pgvector faiss; do
  if [[ $engine == qdrant ]]; then
    repeat=10
  else
    repeat=3
  fi
  report="$tmp/$engine.json"
  args=(run --dataset "$DATASET" --engine "$engine" --repeat "$repeat" --k 10
    --expect-sha256 "$EXPECT_SHA256" --out "$report"
    --engine-url "$QDRANT_URL" --database-url "$DATABASE_URL")
  if [[ $engine == faiss ]]; then
    args+=(--faiss-python "$FAISS_PYTHON")
  fi
  engine_status=0
  engine_out=$("$KNN_AUDIT" "${args[@]}" 2>&1) || engine_status=$?
  echo "$engine_out"
  [[ ${engine_status:-0} -eq 0 ]]

  jq -e --argjson repeat "$repeat" --arg sha "$EXPECT_SHA256" '
    .schema == "knn-audit/v1" and .repeat == $repeat and (.runs | length) == $repeat
    and .k == 10 and .dataset.base_sha256 == $sha
    and .summary.cases == 1616 and (.categories | length) == 21
    and .summary.passed + .summary.failed + .summary.order_only == .summary.cases
    and ([.categories[].failed] | add) == .summary.failed
    and ([.categories[].order_only] | add) == .summary.order_only
    and .summary.failed_all <= .summary.failed and .summary.errors <= .summary.failed
    and ([.runs[].failed] | add) >= .summary.failed
    and ([.runs[].order_only] | add) >= .summary.order_only
    and all(.failures[]; (.severity == "mismatch" or .severity == "error" or .severity == "order_only")
      and ((.severity == "order_only" | not) or ((.missing_ids | length) == 0 and (.extra_ids | length) == 0)))
  ' "$report"
  case $engine in
    reference) jq -e '.summary.failed == 0 and .summary.order_only == 0 and .summary.errors == 0' "$report" ;;
    pgvector) jq -e '.summary.failed == 542 and .summary.order_only == 4 and .summary.errors == 0' "$report" ;;
    faiss) jq -e '.summary.failed == 541 and .summary.order_only == 4 and .summary.errors == 31' "$report" ;;
    qdrant) jq -e '([.runs[].failed] | min) >= 450 and ([.runs[].failed] | max) <= 650
      and .summary.failed >= 800 and .summary.failed <= 1000
      and .summary.failed_all >= 150 and .summary.failed_all <= 250
      and .summary.unstable >= 700 and .summary.unstable <= 900
      and .summary.order_only >= 180 and .summary.order_only <= 240 and .summary.errors == 0' "$report" ;;
  esac

  version=$(jq -r '.engine.version' "$report")
  if [[ ! $version =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "unsafe engine version: $version" >&2
    exit 2
  fi
  if [[ $engine == qdrant ]]; then
    card="$scorecards/qdrant-$version-repeat10-$QDRANT_GROUP.json"
  else
    card="$scorecards/$engine-$version.json"
  fi
  publish_status=0
  rm -f "$card"
  publish_out=$("$KNN_AUDIT" publish --input "$report" --out "$card" 2>&1) || publish_status=$?
  echo "$publish_out"
  [[ ${publish_status:-0} -eq 0 ]]
  jq -e --slurpfile report "$report" '
    .schema == "knn-audit-scorecard/v1" and .summary == $report[0].summary
    and .engine == $report[0].engine and .tool == $report[0].tool
    and .tool.name == "knn-audit" and (.tool.version | type) == "string" and (.tool.version | length) > 0
    and (has("created_at") | not) and (.dataset | has("root") | not)
    and ([paths | select(.[-1] == "query" or .[-1] == "root" or .[-1] == "created_at")] | length) == 0
  ' "$card"
done
