#!/usr/bin/env bash
set -euo pipefail

KNN_AUDIT=${KNN_AUDIT:-./target/release/knn-audit}
DATASET=${DATASET:-.data/errpedia}
QDRANT_URL=${QDRANT_URL:-http://127.0.0.1:6333}
DATABASE_URL=${DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/postgres}
FAISS_PYTHON=${FAISS_PYTHON:-.venv-faiss/bin/python}

for _ in $(seq 1 60); do
  if curl -fsS "$QDRANT_URL/" >/dev/null; then
    break
  fi
  sleep 1
done
curl -fsS "$QDRANT_URL/" >/dev/null

verify=$("$KNN_AUDIT" dataset verify --dataset "$DATASET" --all)
grep -q '0/1616' <<<"$verify"
echo "$verify"

"$KNN_AUDIT" run --dataset "$DATASET" --engine reference --repeat 1 \
  --out results/reference-repeat1.json >/dev/null
jq -e '.summary.failed == 0' results/reference-repeat1.json >/dev/null

mkdir -p results/scorecards

for engine in reference qdrant pgvector faiss; do
  report="results/$engine.json"
  scorecard="results/scorecards/$engine.json"
  args=(run --dataset "$DATASET" --engine "$engine" --repeat 3 --out "$report"
    --engine-url "$QDRANT_URL" --database-url "$DATABASE_URL")
  if [[ $engine == faiss ]]; then
    args+=(--faiss-python "$FAISS_PYTHON")
  fi
  "$KNN_AUDIT" "${args[@]}" >/dev/null
  jq -e '.schema == "knn-audit/v1" and .summary.cases == 1616 and (.runs | length) == 3' "$report" >/dev/null
  case $engine in
    pgvector) jq -e '.summary.failed == 546 and .summary.unstable == 0' "$report" >/dev/null ;;
    faiss) jq -e '.summary.failed == 545 and .summary.errors == 31' "$report" >/dev/null ;;
    qdrant) jq -e '.summary.failed > 0 and .summary.failed_all > 0 and .summary.unstable > 0' "$report" >/dev/null ;;
  esac
  "$KNN_AUDIT" publish --input "$report" --out "$scorecard"
  jq -e '.schema == "knn-audit-scorecard/v1"' "$scorecard" >/dev/null
  jq -e '[paths | select(.[-1] == "query")] | length == 0' "$scorecard" >/dev/null
  jq -e '.dataset | has("root") | not' "$scorecard" >/dev/null
done
