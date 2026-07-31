#!/bin/bash
# Exclusive mutex for performance measurement on this machine.
#
# WHY: S43 runs three investigations concurrently in separate worktrees, on ONE
# M4 Pro with TWO SME units and one thermal envelope. Two timed runs overlapping
# is not "a bit noisy" — it is the measurement-rule-14 failure that already put a
# thermal artifact (`loadcost.c`'s 1864 L1 row) into the published record. A
# concurrent `cargo build` is just as fatal as a concurrent benchmark.
#
# CONTRACT: every HEAVY command — timed runs AND `cargo build`/`clang` — in every
# worktree, goes through this wrapper.
#   benches/perflock.sh <command...>
# It waits until it holds the lock AND the machine is quiet, then runs <command>
# and releases. Exit status is the command's.
#
# "BUILDS TOO" IS NOT BELT-AND-BRACES — it closes a real hole. The quiet check runs
# ONCE, at acquire. If a build starts *during* someone's 20-minute measurement, the
# measurement is poisoned and nothing notices. That happened: a threaded A/B whose
# OFF arm should spread ~3.7% spread 53.9–92.7 ms, a ~70% noise floor, and the run
# was void. A build that takes the lock cannot start inside another agent's window.
#
# The lock path is absolute and shared across worktrees on purpose — a per-worktree
# lock would mutex nothing.
set -u

# TIMEOUTS ARE BOUNDED BELOW THE AGENT WATCHDOG, ON PURPOSE.
# A blocking wait makes no stream progress, so an agent parked here for 600s gets
# killed by the harness watchdog — the mutex then starves exactly the agents it
# protects. That happened. So: give up at 240s with exit 75, hand control back,
# and let the caller do other work and retry. NEVER raise these above ~300 unless
# the caller is not an agent.
LOCK="${MAPAL_PERF_LOCK:-/tmp/mapal-perf.lock}"
WAIT_LOCK="${MAPAL_PERF_WAIT:-240}"    # max seconds to wait for the lock, then EX_TEMPFAIL
WAIT_QUIET="${MAPAL_PERF_QUIET:-120}"  # max seconds to wait for the machine to go quiet
SETTLE="${MAPAL_PERF_SETTLE:-3}"       # post-quiet settle, seconds

[ $# -gt 0 ] || { echo "perflock: usage: perflock.sh <command...>" >&2; exit 2; }

# --- acquire: mkdir is atomic on POSIX; the pid file makes staleness detectable.
waited=0
while ! mkdir "$LOCK" 2>/dev/null; do
  owner=$(cat "$LOCK/pid" 2>/dev/null || echo "")
  if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null; then
    echo "perflock: stale lock from dead pid $owner, reclaiming" >&2
    rm -rf "$LOCK"
    continue
  fi
  if [ "$waited" -ge "$WAIT_LOCK" ]; then
    echo "perflock: BUSY — pid ${owner:-?} still holds the lock after ${WAIT_LOCK}s." >&2
    echo "perflock: exit 75 = RETRY LATER, NOT A FAILURE. Your command did NOT run." >&2
    echo "perflock: go do non-measurement work (read, code, write up) and try again." >&2
    exit 75
  fi
  [ "$((waited % 60))" -eq 0 ] && echo "perflock: waiting for pid ${owner:-?} (${waited}s)" >&2
  sleep 5; waited=$((waited + 5))
done
echo $$ > "$LOCK/pid"
trap 'rm -rf "$LOCK"' EXIT INT TERM

# --- wait for quiet: holding the lock does not stop another worktree's compiler.
# Anything CPU-heavy poisons an SME timing run, so wait it out rather than measure
# through it. Our own descendants are excluded or we would deadlock on ourselves.
busy_pids() {
  ps -Ao pid=,ppid=,comm= | awk -v self=$$ '
    { cmd = $3; sub(/.*\//, "", cmd) }
    cmd ~ /^(cargo|rustc|clang|clang\+\+|cc1|cc1plus|ld|lld|ld64|swift-frontend|python3\.[0-9]+|conftest)$/ \
      && $1 != self && $2 != self { print $1 }'
}
qwaited=0
while :; do
  busy=$(busy_pids)
  [ -z "$busy" ] && break
  if [ "$qwaited" -ge "$WAIT_QUIET" ]; then
    if [ "${MAPAL_PERF_FORCE:-0}" = "1" ]; then
      echo "perflock: FORCED past a busy machine (pids: $(echo $busy | tr '\n' ' ')). TREAT THIS RUN AS SUSPECT." >&2
      break
    fi
    # Refusing beats publishing a poisoned number: this project already put one
    # thermal artifact into the record (S42's 1864 L1 ceiling). Release and retry.
    echo "perflock: BUSY — build activity has not drained after ${WAIT_QUIET}s (pids: $(echo $busy | tr '\n' ' '))." >&2
    echo "perflock: exit 75 = RETRY LATER, NOT A FAILURE. Your command did NOT run." >&2
    echo "perflock: measuring through another agent's build would poison the result." >&2
    echo "perflock: set MAPAL_PERF_FORCE=1 to override, and mark the run SUSPECT if you do." >&2
    rm -rf "$LOCK"; trap - EXIT INT TERM
    exit 75
  fi
  [ "$qwaited" -eq 0 ] && echo "perflock: lock held; waiting for build activity to drain" >&2
  sleep 5; qwaited=$((qwaited + 5))
done
sleep "$SETTLE"

echo "perflock: acquired (waited ${waited}s lock / ${qwaited}s quiet) — running: $*" >&2
"$@"
status=$?
echo "perflock: released (exit $status)" >&2
exit $status
