#!/bin/bash
###########################################################################
# Run olympia-g3's fixtures against the rhorri binary.
#
# The fixtures stay in the olympia-g3 checkout and run unmodified. Each
# check.sh finds its repo root from its own path and runs the engine at
# ${ROOT}/build/${OLYMPIA_PRESET}/olympia-g3, so this script builds a shadow
# root in tmp/harness/root:
#
#   - every top-level entry of olympia-g3 is a symlink to the real one,
#     except build/ and run/;
#   - build/rhorri/olympia-g3 is the rhorri binary, and the preset's other
#     tools (olyscript-g3, ...) link to olympia-g3's C build;
#   - run/ is a real directory, so the golden flow writes its turn here.
#
# The scripts are then run through the shadow path with OLYMPIA_PRESET=rhorri.
# They are never given --update, the only mode that writes into the fixture
# directories, so the olympia-g3 checkout is not modified.
#
# Usage: harness/run.sh [FIXTURE ...]
#
#   FIXTURE is "golden" (run/olympia-g3.sh + tests/olympia/golden_check.sh)
#   or a script under tests/, e.g. olympia/regress/rng-selftest/check.sh.
#   A directory name means its check.sh. With no FIXTURE, runs the golden
#   flow and every tests/olympia/regress/*/check*.sh.
#
# Environment:
#   OLYMPIA_G3        the olympia-g3 checkout (~/Software/playbymail/olympia-g3)
#   OLYMPIA_C_PRESET  the C build the other tools come from (debug)
#   ENGINE            run this engine binary instead of rhorri's, e.g. the
#                     C build, to check the harness itself
#
# Prints PASS or FAIL per fixture; logs go to tmp/harness/logs/. Exits 1 if
# a fixture listed in harness/passing.txt fails, 0 otherwise. A fixture that
# passes but is not listed is reported so it can be added.
###########################################################################
set -u
unset CDPATH

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RHORRI="$(cd "${HERE}/.." && pwd)"
OLYMPIA_G3="${OLYMPIA_G3:-${HOME}/Software/playbymail/olympia-g3}"
C_PRESET="${OLYMPIA_C_PRESET:-debug}"
PASSING="${HERE}/passing.txt"
WORK="${RHORRI}/tmp/harness"
SHADOW="${WORK}/root"
LOGS="${WORK}/logs"

[ -d "${OLYMPIA_G3}/tests/olympia" ] || { echo "error: no olympia-g3 checkout at ${OLYMPIA_G3} (set OLYMPIA_G3)"; exit 2; }

if [ -n "${ENGINE:-}" ]; then
	BIN="${ENGINE}"
else
	( cd "${RHORRI}" && cargo build --quiet ) || { echo "error: cargo build failed"; exit 2; }
	BIN="${RHORRI}/target/debug/olympia-g3"
fi
[ -x "${BIN}" ] || { echo "error: no engine binary at ${BIN}"; exit 2; }

# Build the shadow root afresh.
rm -rf "${WORK}"
mkdir -p "${SHADOW}/build/rhorri" "${SHADOW}/run/olympia" "${LOGS}" || exit 2
for entry in "${OLYMPIA_G3}"/* "${OLYMPIA_G3}"/.[!.]*; do
	[ -e "${entry}" ] || continue
	name="$(basename "${entry}")"
	case "${name}" in
	build|run|.git) ;;
	*) ln -s "${entry}" "${SHADOW}/${name}" || exit 2 ;;
	esac
done
ln -s "${OLYMPIA_G3}/run/olympia-g3.sh" "${SHADOW}/run/olympia-g3.sh" || exit 2
for tool in "${OLYMPIA_G3}/build/${C_PRESET}"/*-g3; do
	[ -x "${tool}" ] || continue
	ln -s "${tool}" "${SHADOW}/build/rhorri/$(basename "${tool}")" || exit 2
done
ln -sf "${BIN}" "${SHADOW}/build/rhorri/olympia-g3" || exit 2

# The fixtures to run, as names relative to tests/.
if [ "$#" -eq 0 ]; then
	set -- golden
	for script in "${OLYMPIA_G3}"/tests/olympia/regress/*/check*.sh; do
		set -- "$@" "${script#"${OLYMPIA_G3}/tests/"}"
	done
fi

# Runs one fixture through the shadow root; its output goes to the log.
run_fixture() {
	local name="$1"
	if [ "${name}" = golden ]; then
		"${SHADOW}/run/olympia-g3.sh" && "${SHADOW}/tests/olympia/golden_check.sh"
		return
	fi
	local script="${SHADOW}/tests/${name}"
	[ -f "${script}" ] || { echo "no such fixture: tests/${name}"; return 2; }
	bash "${script}"
}

# A directory names its check.sh, as harness/passing.txt lists it.
canonical() {
	local name="${1%/}"
	if [ "${name}" != golden ] && [ -d "${OLYMPIA_G3}/tests/${name}" ]; then
		name="${name}/check.sh"
	fi
	printf '%s' "${name}"
}

listed() { grep -qxF "$1" "${PASSING}" 2>/dev/null; }

pass=0 fail=0 regressed="" fresh=""
for name in "$@"; do
	name="$(canonical "${name}")"
	log="${LOGS}/$(printf '%s' "${name}" | tr '/' '_').log"
	if ( cd "${WORK}" && OLYMPIA_PRESET=rhorri run_fixture "${name}" ) </dev/null >"${log}" 2>&1; then
		echo "PASS  ${name}"
		pass=$((pass + 1))
		listed "${name}" || fresh="${fresh} ${name}"
	else
		echo "FAIL  ${name}"
		fail=$((fail + 1))
		listed "${name}" && regressed="${regressed} ${name}"
	fi
done

echo
echo "${pass} passed, ${fail} failed; logs in ${LOGS}"
for name in ${fresh}; do
	echo "new pass, add to harness/passing.txt: ${name}"
done
if [ -n "${regressed}" ]; then
	for name in ${regressed}; do
		echo "REGRESSION (listed in harness/passing.txt): ${name}"
	done
	exit 1
fi
exit 0
