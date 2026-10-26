#!/bin/bash
###########################################################################
# Compare rhorri with the C engine on one turn (-r -S).
#
# Runs `olympia-g3 -r -l ./lib -S` with both engines on identical copies of
# the shared fixture world (world.tgz, no orders queued), or of a regress
# fixture's saved game, and diffs the results: each engine's
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
# Usage: harness/turn.sh [fixture]
#
# With a fixture name (a directory under olympia-g3's tests/olympia/regress/,
# e.g. order-unrecognized), the turn runs on that fixture's saved game, built
# as its check.sh builds it (tests/olympia/fixtures/README.md): its frozen
# scenario.patch over world.tgz, its scenario.tgz, or else world.tgz itself;
# then its SEED, if check.sh sets one, is written to lib/randseed. Only
# one-turn fixtures are supported. Both engines get the turn flags every
# check.sh passes (-r -l ./lib -S test-use-const-report-date) but not the
# dump flags (--coverage, --state-export, ...): those write files outside
# lib/ and are not ported yet, so rhorri would stop on them.
#
# Environment:
#   OLYMPIA_G3        the olympia-g3 checkout (~/Software/playbymail/olympia-g3)
#   OLYMPIA_C_PRESET  the C build to compare against (debug)
#   ENGINE            run this engine binary instead of rhorri's, e.g. the
#                     C build, to check the script itself
#
# Work goes in tmp/turn (tmp/turn-<fixture> for a fixture), and is kept for
# inspection: c/ and r/ hold each engine's run. Prints the differences; exits 1 if there are any.
###########################################################################
set -u
unset CDPATH

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RHORRI="$(cd "${HERE}/.." && pwd)"
ROOT="${OLYMPIA_G3:-${HOME}/Software/playbymail/olympia-g3}"
C="${ROOT}/build/${OLYMPIA_C_PRESET:-debug}/olympia-g3"
FIXTURE="${1:-}"
W="${RHORRI}/tmp/turn${FIXTURE:+-${FIXTURE}}"
WORLD="${ROOT}/tests/olympia/fixtures/world.tgz"
FX="${ROOT}/tests/olympia/regress/${FIXTURE}"

[ $# -le 1 ] || { echo "usage: harness/turn.sh [fixture]"; exit 2; }
[ -x "${C}" ] || { echo "error: no C engine at ${C}"; exit 2; }
[ -f "${WORLD}" ] || { echo "error: no fixture world at ${WORLD}"; exit 2; }
if [ -n "${FIXTURE}" ]; then
	case "${FIXTURE}" in */* | .*) echo "error: bad fixture name: ${FIXTURE}"; exit 2 ;; esac
	[ -f "${FX}/check.sh" ] || { echo "error: no fixture at ${FX}"; exit 2; }
	n="$(grep -c -- '-r -l ./lib' "${FX}/check.sh")"
	[ "${n}" -eq 1 ] || { echo "error: ${FIXTURE} is not a one-turn fixture"; exit 2; }
	. "${ROOT}/tests/olympia/fixtures/scenario.sh"
fi
if [ -n "${ENGINE:-}" ]; then
	R="${ENGINE}"
else
	( cd "${RHORRI}" && cargo build --release --quiet ) || { echo "error: cargo build failed"; exit 2; }
	R="${RHORRI}/target/release/olympia-g3"
fi
[ -x "${R}" ] || { echo "error: no engine binary at ${R}"; exit 2; }

# Builds the saved game the turn runs on in $1.
unpack() {
	local seed
	if [ -z "${FIXTURE}" ]; then
		tar zxf "${WORLD}" -C "$1"
		return
	fi
	if [ -f "${FX}/scenario.patch" ]; then
		scenario_unpack "${FX}" "$1" || return
	elif [ -f "${FX}/scenario.tgz" ]; then
		tar zxf "${FX}/scenario.tgz" -C "$1" || return
	else
		tar zxf "${WORLD}" -C "$1" || return
	fi
	seed="$(sed -n "s/^SEED='\([^']*\)'.*/\1/p" "${FX}/check.sh")"
	if [ -n "${seed}" ]; then
		printf '%s\n' "${seed}" >"$1/lib/randseed"
	fi
}

rm -rf "${W:?}"
mkdir -p "${W}/c" "${W}/r" || exit 2
unpack "${W}/c" && unpack "${W}/r" || exit 2

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
	echo "PASS  turn on ${FIXTURE:-world.tgz}"
else
	echo "work kept in ${W}"
fi
exit "${status}"
