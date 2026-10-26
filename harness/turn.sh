#!/bin/bash
###########################################################################
# Compare rhorri with the C engine on one turn (-r -S, no orders queued).
#
# Runs `olympia-g3 -r -l ./lib -S` with both engines on identical copies of
# the shared fixture world (world.tgz) and diffs the results: each engine's
# exit status, stdout and stderr, and every lib/ file except the stage 2
# output (reports and player files), which are removed from both copies
# before the diff:
#
#   email forward factions players totimes times_* log/ save/
#
# Everything else is compared: the files a turn changes (loc, item, misc,
# master, system, unform, fact/*, orders/*) and those it leaves alone (gate,
# road, ship, skill, randseed, lore, startloc). Stage 1's exit check (#5).
#
# Usage: harness/turn.sh
#
# Environment:
#   OLYMPIA_G3        the olympia-g3 checkout (~/Software/playbymail/olympia-g3)
#   OLYMPIA_C_PRESET  the C build to compare against (debug)
#   ENGINE            run this engine binary instead of rhorri's, e.g. the
#                     C build, to check the script itself
#
# Work goes in tmp/turn, and is kept for inspection: c/ and r/ hold each
# engine's run. Prints the differences; exits 1 if there are any.
###########################################################################
set -u
unset CDPATH

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RHORRI="$(cd "${HERE}/.." && pwd)"
ROOT="${OLYMPIA_G3:-${HOME}/Software/playbymail/olympia-g3}"
C="${ROOT}/build/${OLYMPIA_C_PRESET:-debug}/olympia-g3"
W="${RHORRI}/tmp/turn"
WORLD="${ROOT}/tests/olympia/fixtures/world.tgz"

[ -x "${C}" ] || { echo "error: no C engine at ${C}"; exit 2; }
[ -f "${WORLD}" ] || { echo "error: no fixture world at ${WORLD}"; exit 2; }
if [ -n "${ENGINE:-}" ]; then
	R="${ENGINE}"
else
	( cd "${RHORRI}" && cargo build --release --quiet ) || { echo "error: cargo build failed"; exit 2; }
	R="${RHORRI}/target/release/olympia-g3"
fi
[ -x "${R}" ] || { echo "error: no engine binary at ${R}"; exit 2; }

rm -rf "${W:?}"
mkdir -p "${W}/c" "${W}/r" || exit 2
tar zxf "${WORLD}" -C "${W}/c" && tar zxf "${WORLD}" -C "${W}/r" || exit 2

( cd "${W}/c" && "${C}" -r -l ./lib -S test-use-const-report-date </dev/null >out 2>err ); ce=$?
( cd "${W}/r" && "${R}" -r -l ./lib -S test-use-const-report-date </dev/null >out 2>err ); re=$?

for side in c r; do
	( cd "${W}/${side}/lib" && rm -rf email forward factions players totimes times_* log save ) || exit 2
done

status=0
if [ "${ce}" -ne "${re}" ]; then
	echo "FAIL  exit status: C=${ce} rhorri=${re}"
	status=1
fi
for f in out err; do
	if ! cmp -s "${W}/c/${f}" "${W}/r/${f}"; then
		echo "FAIL  std${f}:"
		diff "${W}/c/${f}" "${W}/r/${f}" | head -15
		status=1
	fi
done
if ! d="$(cd "${W}" && diff -rq c/lib r/lib 2>&1)"; then
	echo "FAIL  lib/:"
	printf '%s\n' "${d}"
	status=1
fi

if [ "${status}" -eq 0 ]; then
	echo "PASS  turn on world.tgz"
else
	echo "work kept in ${W}"
fi
exit "${status}"
