#!/usr/bin/env bash
set -uo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

files=(.github/workflows/ci.yml .github/workflows/integration.yml scripts/ci-audit.sh README.md results/README.md)
missing=0

need() {
  if ! rg -q "$2" "$1"; then
    printf 'MISSING %s: %s\n' "$1" "$2"
    missing=1
  fi
}

for file in "${files[@]}"; do
  if [[ ! -f $file ]]; then
    printf 'MISSING file: %s\n' "$file"
    missing=1
  fi
done

# Every pin must survive somewhere in the CI/docs set.
pins=(
  'ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06'
  'c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb'
  'sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277'
  'sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5'
  '1\.100\.0'
  '3\.12'
  '1,?616'
  '21 cate'
  'top-?10|--k 10'
  '\-\-repeat 10'
  '\-\-repeat 3'
  'check-pins\.sh'
)
for pin in "${pins[@]}"; do
  if ! rg -q "$pin" "${files[@]}"; then
    printf 'MISSING from all pinned files: %s\n' "$pin"
    missing=1
  fi
done

need .github/workflows/ci.yml 'check-pins\.sh'
need .github/workflows/ci.yml '1\.100\.0'
need .github/workflows/integration.yml 'check-pins\.sh'
need .github/workflows/integration.yml 'ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06'
need .github/workflows/integration.yml 'sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277'
need .github/workflows/integration.yml 'sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5'
need .github/workflows/integration.yml '1\.100\.0'
need .github/workflows/integration.yml '3\.12'
need .github/workflows/integration.yml '\-\-repeat 10'
need .github/workflows/integration.yml '\-\-repeat 3'
need scripts/ci-audit.sh 'ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06'
need scripts/ci-audit.sh 'c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb'
need scripts/ci-audit.sh '\-\-expect-sha256'
need scripts/ci-audit.sh '1,?616'
need scripts/ci-audit.sh '21 cate'
need scripts/ci-audit.sh '\-\-k 10'
need scripts/ci-audit.sh '\-\-repeat 10'
need scripts/ci-audit.sh '\-\-repeat 3'
need README.md '^Audit '
need README.md 'ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06'
need README.md 'c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb'
need README.md 'sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277'
need README.md 'sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5'
need README.md '1,?616'
need README.md '21 cate'
need README.md 'top-?10'
need README.md '\-\-expect-sha256'
need README.md '\-\-jobs'
need README.md '\-\-repeat 10'
need README.md '\-\-repeat 3'
need results/README.md 'ba9bfbb7649b34d0c7619e2cf10bf6acb9fd6d06'
need results/README.md 'c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb'
need results/README.md 'sha256:f60765106f26981dabc593d88fe4d0c8138893442c7c59d62257390891b65277'
need results/README.md 'sha256:89879f41d564f67b0c1462a491d16fcbc6de39ac10db17e7a4bba2138a5e0ba5'
need results/README.md '1\.100\.0'
need results/README.md '3\.12'
need results/README.md '1,?616'
need results/README.md '21 cate'
need results/README.md 'top-?10'
need results/README.md '\-\-repeat 10'
need results/README.md '\-\-repeat 3'

exit "$missing"
