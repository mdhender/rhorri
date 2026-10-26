#!/bin/bash
###########################################################################
# Compare rhorri with the C engine on load + save (-S, no turn).
#
# For the shared fixture world and every regress fixture's saved game,
# before and after a C turn, runs `olympia-g3 -l ./lib -S` with both
# engines on identical copies and diffs the whole directories: every lib/
# file, plus each engine's stdout and stderr.
#
# Usage: harness/roundtrip.sh
#
# Environment:
#   OLYMPIA_G3        the olympia-g3 checkout (~/Software/playbymail/olympia-g3)
#   OLYMPIA_C_PRESET  the C build to compare against (debug)
#
# Work goes in tmp/roundtrip. Prints a line per failure; exits 1 if any.
###########################################################################
set -u
unset CDPATH

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RHORRI="$(cd "${HERE}/.." && pwd)"
ROOT="${OLYMPIA_G3:-${HOME}/Software/playbymail/olympia-g3}"
C="${ROOT}/build/${OLYMPIA_C_PRESET:-debug}/olympia-g3"
R="${RHORRI}/target/release/olympia-g3"
W="${RHORRI}/tmp/roundtrip"

[ -x "${C}" ] || { echo "error: no C engine at ${C}"; exit 2; }
( cd "${RHORRI}" && cargo build --release --quiet ) || { echo "error: cargo build failed"; exit 2; }

rm -rf "${W:?}"
mkdir -p "${W}/fx" || exit 2
# shellcheck source=/dev/null
. "${ROOT}/tests/olympia/fixtures/scenario.sh"

pass=0 fail=0

# Saves a copy of the game in $2 with each engine and compares.
compare() {
	local name="$1" src="$2" ce re d
	rm -rf "${W:?}/c" "${W:?}/r"
	cp -R "${src}" "${W}/c" && cp -R "${src}" "${W}/r" || exit 2
	( cd "${W}/c" && "${C}" -l ./lib -S test-use-const-report-date </dev/null >out 2>err ); ce=$?
	( cd "${W}/r" && "${R}" -l ./lib -S test-use-const-report-date </dev/null >out 2>err ); re=$?
	if [ "${ce}" -ne "${re}" ]; then
		echo "FAIL  ${name}: exit C=${ce} rhorri=${re}"
		tail -2 "${W}/r/err"
		fail=$((fail + 1))
	elif d="$(diff -r "${W}/c" "${W}/r" 2>&1)"; then
		pass=$((pass + 1))
	else
		echo "FAIL  ${name}"
		printf '%s\n' "${d}" | head -15
		fail=$((fail + 1))
	fi
}

mkdir "${W}/fx/world" && tar zxf "${SCENARIO_WORLD}" -C "${W}/fx/world" || exit 2
compare world "${W}/fx/world"

for fx in "${ROOT}"/tests/olympia/regress/*/; do
	n="$(basename "${fx}")"
	[ -f "${fx}/scenario.patch" ] || continue
	mkdir "${W}/fx/${n}" || exit 2
	scenario_unpack "${fx}" "${W}/fx/${n}" >/dev/null || { echo "skip  ${n}: unpack failed"; continue; }
	compare "${n} (before the turn)" "${W}/fx/${n}"
	if ( cd "${W}/fx/${n}" && "${C}" -r -l ./lib -S test-use-const-report-date </dev/null >/dev/null 2>&1 ); then
		compare "${n} (after the turn)" "${W}/fx/${n}"
	else
		echo "skip  ${n}: C turn failed"
	fi
done

echo "${pass} passed, ${fail} failed"
[ "${fail}" -eq 0 ]
