#!/usr/bin/env bash
# Run CodeCartographer analysis on the current PR and post a health-delta comment.
#
# Env (all set by action.yml):
#   WORKING_DIR, COMMITS, BASE_SHA, HEAD_SHA
#   FAIL_ON_CYCLE, FAIL_ON_LAYER_VIOLATION, FAIL_ON_REGRESSION, REGRESSION_THRESHOLD
#   GITHUB_TOKEN, REPO, PR_NUMBER, POST_COMMENT, EVENT_NAME, ACTION_PATH
set -euo pipefail

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() { echo "::error::${*}"; exit 1; }

jq_or_default() {
  # jq_or_default <json_file> <query> <default>
  local f="$1" q="$2" d="$3"
  if [[ -f "${f}" ]]; then
    result=$(jq -r "${q} // empty" "${f}" 2>/dev/null || true)
    echo "${result:-${d}}"
  else
    echo "${d}"
  fi
}

delta_arrow() {
  # Print +N or -N with an arrow emoji.
  local val="$1"
  if (( $(echo "${val} > 0.05" | bc -l) )); then
    echo "+${val} ⬆"
  elif (( $(echo "${val} < -0.05" | bc -l) )); then
    echo "${val} ⬇"
  else
    echo "0"
  fi
}

int_delta_arrow() {
  local now="$1" base="$2"
  local d=$(( now - base ))
  if (( d > 0 )); then echo "+${d} ⬆"
  elif (( d < 0 )); then echo "${d} ⬇"
  else echo "0"
  fi
}

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

cd "${WORKING_DIR}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "${TMPDIR}"' EXIT

HEALTH_HEAD="${TMPDIR}/health_head.json"
HEALTH_COMPARE="${TMPDIR}/health_compare.json"
HOTSPOTS="${TMPDIR}/hotspots.json"
CHANGED_FILES="${TMPDIR}/changed_files.txt"

# Shallow-clone guard: health --compare needs the base commit in history.
if [[ -n "${BASE_SHA}" ]]; then
  if ! git cat-file -e "${BASE_SHA}^{commit}" 2>/dev/null; then
    echo "::warning::Base commit ${BASE_SHA} not in local history. Add 'fetch-depth: 0' to your checkout step. Skipping health delta."
    BASE_SHA=""
  fi
fi

EXIT_CODE=0

# ---------------------------------------------------------------------------
# Run analysis
# ---------------------------------------------------------------------------

echo "::group::CodeCartographer health (HEAD)"
if ! codecartographer health --json > "${HEALTH_HEAD}" 2>&1; then
  cat "${HEALTH_HEAD}"
  die "codecartographer health failed"
fi
cat "${HEALTH_HEAD}"
echo "::endgroup::"

if [[ -n "${BASE_SHA:-}" ]]; then
  echo "::group::CodeCartographer health --compare ${BASE_SHA}"
  if ! codecartographer health --compare "${BASE_SHA}" --json > "${HEALTH_COMPARE}" 2>&1; then
    echo "::warning::health --compare failed — delta will be omitted"
    cat "${HEALTH_COMPARE}"
    BASE_SHA=""
  else
    cat "${HEALTH_COMPARE}"
  fi
  echo "::endgroup::"
fi

echo "::group::CodeCartographer hotspots"
codecartographer hotspots --commits "${COMMITS}" --by-author --bus-factor --json \
  > "${HOTSPOTS}" 2>&1 || true
echo "::endgroup::"

# Files changed in this PR (for hotspot cross-reference).
if [[ -n "${BASE_SHA:-}" ]]; then
  git diff --name-only "${BASE_SHA}" "${HEAD_SHA}" > "${CHANGED_FILES}" 2>/dev/null || true
fi

# Snapshot for artifact upload.
echo "::group::CodeCartographer snapshot"
codecartographer snapshot save "${HEAD_SHA:0:12}" 2>&1 || true
echo "::endgroup::"

# ---------------------------------------------------------------------------
# Gate: cycles
# ---------------------------------------------------------------------------

CYCLE_COUNT=$(jq_or_default "${HEALTH_HEAD}" '.cycle_count' '0')
if [[ "${FAIL_ON_CYCLE}" == "true" && "${CYCLE_COUNT}" -gt 0 ]]; then
  echo "::error::${CYCLE_COUNT} dependency cycle(s) detected (fail-on-cycle: true)"
  EXIT_CODE=1
fi

# ---------------------------------------------------------------------------
# Gate: layer violations
# ---------------------------------------------------------------------------

VIOLATION_COUNT=$(jq_or_default "${HEALTH_HEAD}" '.layer_violation_count' '0')
if [[ "${FAIL_ON_LAYER_VIOLATION}" == "true" && "${VIOLATION_COUNT}" -gt 0 ]]; then
  echo "::error::${VIOLATION_COUNT} layer violation(s) detected (fail-on-layer-violation: true)"
  EXIT_CODE=1
fi

# ---------------------------------------------------------------------------
# Gate: health regression
# ---------------------------------------------------------------------------

SCORE_HEAD=$(jq_or_default "${HEALTH_HEAD}" '.health_score' '0')
HEALTH_DELTA="0"

if [[ -f "${HEALTH_COMPARE}" ]]; then
  SCORE_BASE=$(jq_or_default "${HEALTH_COMPARE}" '.base.score' '0')
  HEALTH_DELTA=$(echo "${SCORE_HEAD} - ${SCORE_BASE}" | bc -l | xargs printf "%.1f")
  if [[ "${FAIL_ON_REGRESSION}" == "true" ]]; then
    DROP=$(echo "${HEALTH_DELTA} < 0" | bc -l)
    if [[ "${DROP}" == "1" ]]; then
      ABS_DROP=$(echo "${HEALTH_DELTA} * -1" | bc -l)
      EXCEEDS=$(echo "${ABS_DROP} >= ${REGRESSION_THRESHOLD}" | bc -l)
      if [[ "${EXCEEDS}" == "1" ]]; then
        echo "::error::Health regressed by ${ABS_DROP} points (threshold: ${REGRESSION_THRESHOLD}, fail-on-regression: true)"
        EXIT_CODE=1
      fi
    fi
  fi
fi

# ---------------------------------------------------------------------------
# Outputs for downstream steps
# ---------------------------------------------------------------------------

{
  echo "health_score=${SCORE_HEAD}"
  echo "health_delta=${HEALTH_DELTA}"
  echo "cycle_count=${CYCLE_COUNT}"
} >> "${GITHUB_OUTPUT}"

# ---------------------------------------------------------------------------
# Build comment body
# ---------------------------------------------------------------------------

build_comment() {
  local score_head bridge_head cycle_head god_head violations_head
  score_head=$(jq_or_default "${HEALTH_HEAD}" '.health_score' 'N/A')
  bridge_head=$(jq_or_default "${HEALTH_HEAD}" '.bridge_count' 'N/A')
  cycle_head=$(jq_or_default "${HEALTH_HEAD}" '.cycle_count' 'N/A')
  god_head=$(jq_or_default "${HEALTH_HEAD}" '.god_module_count' 'N/A')
  violations_head=$(jq_or_default "${HEALTH_HEAD}" '.layer_violation_count' 'N/A')

  # Score badge color heuristic.
  local score_icon="🟢"
  if (( $(echo "${score_head} < 60" | bc -l 2>/dev/null || echo 0) )); then
    score_icon="🔴"
  elif (( $(echo "${score_head} < 80" | bc -l 2>/dev/null || echo 0) )); then
    score_icon="🟡"
  fi

  cat <<COMMENT
<!-- codecartographer-action-comment -->
## ${score_icon} CodeCartographer Health — \`${score_head}/100\`
COMMENT

  # Delta table (only when compare data is available).
  if [[ -f "${HEALTH_COMPARE}" ]]; then
    local score_base bridge_base cycle_base god_base violations_base
    score_base=$(jq_or_default "${HEALTH_COMPARE}" '.base.score' 'N/A')
    bridge_base=$(jq_or_default "${HEALTH_COMPARE}" '.base.bridges' 'N/A')
    cycle_base=$(jq_or_default "${HEALTH_COMPARE}" '.base.cycles' 'N/A')
    god_base=$(jq_or_default "${HEALTH_COMPARE}" '.base.god_modules' 'N/A')
    violations_base=$(jq_or_default "${HEALTH_COMPARE}" '.base.layer_violations' 'N/A')

    local score_d bridge_d cycle_d god_d violations_d
    score_d=$(delta_arrow "${HEALTH_DELTA}")
    bridge_d=$(int_delta_arrow "${bridge_head}" "${bridge_base}" 2>/dev/null || echo "—")
    cycle_d=$(int_delta_arrow "${cycle_head}" "${cycle_base}" 2>/dev/null || echo "—")
    god_d=$(int_delta_arrow "${god_head}" "${god_base}" 2>/dev/null || echo "—")
    violations_d=$(int_delta_arrow "${violations_head}" "${violations_base}" 2>/dev/null || echo "—")

    cat <<COMMENT

| Metric | Base | Head | Delta |
|--------|-----:|-----:|------:|
| Health Score | ${score_base} | ${score_head} | ${score_d} |
| Bridges | ${bridge_base} | ${bridge_head} | ${bridge_d} |
| Cycles | ${cycle_base} | ${cycle_head} | ${cycle_d} |
| God Modules | ${god_base} | ${god_head} | ${god_d} |
| Layer Violations | ${violations_base} | ${violations_head} | ${violations_d} |
COMMENT
  else
    cat <<COMMENT

| Metric | Value |
|--------|------:|
| Health Score | ${score_head} |
| Bridges | ${bridge_head} |
| Cycles | ${cycle_head} |
| God Modules | ${god_head} |
| Layer Violations | ${violations_head} |
COMMENT
  fi

  # Gate failure notices.
  if [[ "${CYCLE_COUNT}" -gt 0 ]]; then
    echo ""
    echo "> ⚠️ **${CYCLE_COUNT} dependency cycle(s) detected.** Run \`codecartographer health\` locally to see details."
  fi
  if [[ "${VIOLATION_COUNT}" -gt 0 ]]; then
    echo ""
    echo "> ⚠️ **${VIOLATION_COUNT} layer violation(s).** Run \`codecartographer layers validate\` locally."
  fi

  # Hotspots touched by this PR.
  if [[ -f "${HOTSPOTS}" && -f "${CHANGED_FILES}" ]]; then
    local touched_hotspots
    # Cross-reference changed files against hotspot list.
    touched_hotspots=$(jq -r '.hotspots[] | [.path, (.score | tostring), .severity, (.owner // "—"), (.bus_factor // "—" | tostring)] | @tsv' \
      "${HOTSPOTS}" 2>/dev/null | \
      while IFS=$'\t' read -r path score sev owner bf; do
        if grep -qxF "${path}" "${CHANGED_FILES}" 2>/dev/null; then
          printf "| \`%s\` | %s | %s | %s | %s |\n" "${path}" "${score}" "${sev}" "${owner}" "${bf}"
        fi
      done | head -10)

    if [[ -n "${touched_hotspots}" ]]; then
      cat <<COMMENT

**Hotspots touched by this PR:**

| File | Score | Severity | Owner | Authors |
|------|------:|----------|-------|--------:|
${touched_hotspots}
COMMENT
    fi
  fi

  cat <<COMMENT

<sub>Generated by [CodeCartographer](https://github.com/anthropics/codecartographer) · commit \`${HEAD_SHA:0:7}\`</sub>
COMMENT
}

# ---------------------------------------------------------------------------
# Write to step summary (always)
# ---------------------------------------------------------------------------

build_comment >> "${GITHUB_STEP_SUMMARY}"

# ---------------------------------------------------------------------------
# Post / update PR comment
# ---------------------------------------------------------------------------

if [[ "${POST_COMMENT}" == "true" && "${EVENT_NAME}" == "pull_request" && -n "${PR_NUMBER:-}" ]]; then
  COMMENT_BODY=$(build_comment)
  MARKER="<!-- codecartographer-action-comment -->"
  API_BASE="https://api.github.com/repos/${REPO}"

  # Find an existing comment from this action (identified by the marker).
  EXISTING_ID=$(curl -fsSL \
    -H "Authorization: Bearer ${GITHUB_TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    "${API_BASE}/issues/${PR_NUMBER}/comments?per_page=100" \
    | jq -r ".[] | select(.body | startswith(\"${MARKER}\")) | .id" \
    | head -1)

  PAYLOAD=$(jq -n --arg body "${COMMENT_BODY}" '{"body": $body}')

  if [[ -n "${EXISTING_ID}" ]]; then
    echo "Updating existing PR comment ${EXISTING_ID}"
    curl -fsSL \
      -X PATCH \
      -H "Authorization: Bearer ${GITHUB_TOKEN}" \
      -H "Accept: application/vnd.github+json" \
      -H "Content-Type: application/json" \
      "${API_BASE}/issues/comments/${EXISTING_ID}" \
      -d "${PAYLOAD}" > /dev/null
  else
    echo "Posting new PR comment"
    curl -fsSL \
      -X POST \
      -H "Authorization: Bearer ${GITHUB_TOKEN}" \
      -H "Accept: application/vnd.github+json" \
      -H "Content-Type: application/json" \
      "${API_BASE}/issues/${PR_NUMBER}/comments" \
      -d "${PAYLOAD}" > /dev/null
  fi
fi

exit ${EXIT_CODE}
