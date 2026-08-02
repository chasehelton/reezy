#!/usr/bin/env bash
# Ad-hoc verification for the epub2audiobook-rs spike A/B harness.
# NOT a test suite -- the spike is throwaway Phase-0 proof code with no tests.
# Asserts: it builds clean, runs, emits 6 real WAVs, and that the normalized
# variant of each pair is SHORTER than the raw variant (the pause-removal claim).
set -uo pipefail

SPIKE=/home/chase/Repos/epub2audiobook-rs/spike
fails=0
check() { if [ "$2" -eq 0 ]; then echo "  PASS  $1"; else echo "  FAIL  $1"; fails=$((fails+1)); fi; }
dur() { ffprobe -v error -show_entries format=duration -of csv=p=0 "$1" 2>/dev/null; }

cd "$SPIKE" || exit 1

echo "== 1. build / fmt / clippy =="
cargo build --release >/dev/null 2>&1; check "cargo build --release" $?
cargo fmt --check >/dev/null 2>&1; check "cargo fmt --check clean" $?
cargo clippy --release -- -D warnings >/dev/null 2>&1; check "cargo clippy -D warnings clean" $?

echo "== 2. run A/B harness (regenerates artifacts) =="
OUT=$(cargo run --release 2>/dev/null); rc=$?
check "cargo run --release exits 0" $rc

echo "== 3. no dropped phonemes with kokoro-en-v0_19 =="
WARN=$(cargo run --release 2>&1 >/dev/null | grep -ci "unknown phoneme")
[ "$WARN" -eq 0 ]; check "zero 'unknown phoneme' warnings (count: $WARN)" $?

echo "== 4. all six WAVs exist and are real audio =="
for f in ab-abbrev-raw ab-abbrev-norm ab-titles-raw ab-titles-norm ab-numbers-raw ab-numbers-norm; do
  w="$SPIKE/$f.wav"
  if [ ! -f "$w" ]; then check "$f.wav exists" 1; continue; fi
  D=$(dur "$w"); R=$(ffprobe -v error -select_streams a:0 -show_entries stream=sample_rate -of csv=p=0 "$w" 2>/dev/null)
  M=$(ffmpeg -hide_banner -i "$w" -af volumedetect -f null /dev/null 2>&1 | grep mean_volume | grep -oE '\-?[0-9.]+ dB' | grep -oE '\-?[0-9.]+')
  awk -v d="$D" -v r="$R" -v m="$M" 'BEGIN{exit !(d>1 && r==24000 && m>-45 && m<-5)}'
  check "$f.wav real audio (${D}s, ${R}Hz, ${M}dB)" $?
done

echo "== 5. CORE CLAIM: normalized shorter than raw (pause removed) =="
for pair in abbrev titles numbers; do
  RD=$(dur "$SPIKE/ab-$pair-raw.wav"); ND=$(dur "$SPIKE/ab-$pair-norm.wav")
  DELTA=$(awk -v r="$RD" -v n="$ND" 'BEGIN{printf "%.2f", r-n}')
  awk -v r="$RD" -v n="$ND" 'BEGIN{exit !(n<r)}'
  check "$pair: norm ${ND}s < raw ${RD}s (saved ${DELTA}s)" $?
done

echo
[ "$fails" -eq 0 ] && echo "ALL CHECKS PASSED (ad-hoc, not a test suite)" || echo "$fails CHECK(S) FAILED"
exit "$fails"
