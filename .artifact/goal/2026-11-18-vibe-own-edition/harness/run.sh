#!/usr/bin/env bash
# vibe-own dynamic workflow 循环：每轮调用一次执行者（一个 Auto Goal loop），闸门检查后继续。
# 用法：bash .artifact/goal/2026-11-18-vibe-own-edition/harness/run.sh [最大轮数]
# 停止：在 harness 目录创建 STOP 文件。
set -u

HARNESS="$(cd "$(dirname "$0")" && pwd)"
GOAL_DIR="$(dirname "$HARNESS")"
REPO="$(git -C "$HARNESS" rev-parse --show-toplevel)"
GOAL_MD="$GOAL_DIR/goal.md"
SKILL_SCRIPTS="C:/Users/black/.agents/skills/auto-goal/scripts"
RUNS="$HARNESS/runs"
MAX_ITER="${1:-${VK_GOAL_MAX_ITER:-40}}"
MODEL="${VK_GOAL_MODEL:-magpie/codex/gpt-6.2-sol:high}"
ITER_TIMEOUT="${VK_GOAL_ITER_TIMEOUT:-4h}"
OMP_BIN="${VK_OMP_BIN:-C:/Users/black/AppData/Local/Tern/omp/bin/omp.exe}"

mkdir -p "$RUNS"
cd "$REPO" || exit 1
# shellcheck source=/dev/null
source "$HARNESS/env.sh"
[ -f "$RUNS/runs.tsv" ] || printf 'iter\tstart\tend\texit\tstatus\tloop\thead\tprogress\tvalid\n' > "$RUNS/runs.tsv"

state() { python "$SKILL_SCRIPTS/goal_state.py" get "$GOAL_MD" "$1" 2>/dev/null | tr -d '\r'; }
valid() { python "$SKILL_SCRIPTS/validate_goal_file.py" "$GOAL_MD" --strict > /dev/null 2>&1 && echo yes || echo no; }
log() { echo "[$(date '+%F %T')] $*"; }

no_progress=0
invalid=0
for ((i = 1; i <= MAX_ITER; i++)); do
  [ -f "$HARNESS/STOP" ] && { log "发现 STOP 文件，停止"; exit 0; }
  status="$(state status)"
  [ "$status" = "active" ] || { log "goal 状态为 '$status'，停止"; exit 0; }

  head_before="$(git rev-parse HEAD)"
  loop_before="$(state current_loop)"
  start="$(date '+%F %T')"
  logfile="$RUNS/iter-$(date '+%Y%m%d-%H%M%S').log"
  log "第 $i 轮开始：model=$MODEL loop=$loop_before head=${head_before:0:9}"
  timeout --kill-after=5m "$ITER_TIMEOUT" "$OMP_BIN" -p --model="$MODEL" < "$HARNESS/worker-prompt.md" > "$logfile" 2>&1
  rc=$?

  head_after="$(git rev-parse HEAD)"
  loop_after="$(state current_loop)"
  status_after="$(state status)"
  ok="$(valid)"
  if [ "$head_after" != "$head_before" ] || [ "$loop_after" != "$loop_before" ]; then
    progress=yes; no_progress=0
  else
    progress=no; no_progress=$((no_progress + 1))
  fi
  if [ "$ok" = yes ]; then invalid=0; else invalid=$((invalid + 1)); fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$i" "$start" "$(date '+%F %T')" "$rc" "$status_after" \
    "$loop_after" "${head_after:0:9}" "$progress" "$ok" >> "$RUNS/runs.tsv"
  log "第 $i 轮结束：exit=$rc status=$status_after loop=$loop_after progress=$progress valid=$ok"
  tail -n 12 "$logfile"

  [ "$no_progress" -ge 2 ] && { log "连续 2 轮没有进展，停止。查看 $logfile"; exit 2; }
  [ "$invalid" -ge 2 ] && { log "连续 2 轮 goal 校验失败，停止"; exit 3; }
done
log "达到最大轮数 $MAX_ITER，停止"
