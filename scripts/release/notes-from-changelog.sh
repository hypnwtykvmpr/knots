#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: notes-from-changelog.sh <version> [CHANGELOG.md]

Print the changelog section for <version>, where the version may be "0.1.2" or
"v0.1.2". The output is suitable for `gh release --notes-file`.
USAGE
}

version="${1:-}"
changelog="${2:-CHANGELOG.md}"

if [[ -z "${version}" || "${version}" == "-h" || "${version}" == "--help" ]]; then
  usage
  exit 2
fi

if [[ ! -f "${changelog}" ]]; then
  echo "error: changelog not found: ${changelog}" >&2
  exit 1
fi

version="${version#v}"

awk -v version="${version}" '
  function heading_level(line) {
    if (line ~ /^## /) {
      return 2
    }
    if (line ~ /^# /) {
      return 1
    }
    return 0
  }

  function heading_version(line, text) {
    text = line
    sub(/^#+[[:space:]]+/, "", text)
    sub(/[[:space:]].*$/, "", text)
    sub(/^v/, "", text)
    return text
  }

  heading_level($0) > 0 {
    if (capture && heading_level($0) <= level) {
      exit
    }
    if (!capture && heading_version($0) == version) {
      capture = 1
      level = heading_level($0)
      next
    }
  }

  capture {
    lines[++count] = $0
  }

  END {
    while (count > 0 && lines[count] ~ /^[[:space:]]*$/) {
      count--
    }
    for (i = 1; i <= count; i++) {
      print lines[i]
    }
    if (!capture) {
      exit 3
    }
  }
' "${changelog}"
