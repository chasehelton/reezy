# Benchmarks & Phase 0 Findings

## Task 0.3 — Kokoro on AMD Ryzen 5 PRO 7530U

**Date:** 2026-08-02
**Hardware:** AMD Ryzen 5 PRO 7530U, 12 threads, 14GB RAM, CPU-only (no CUDA/ROCm)
**Runtime:** sherpa-rs 0.6.8 -> sherpa-onnx 1.13.4 -> ONNX Runtime, provider=cpu, num_threads=11
**Build:** `cargo run --release`

| Model | RTF | 10h book | ɚ in tokens | Size |
|---|---|---|---|---|
| kokoro-multi-lang-v1_1 | 3.31x / 3.27x | ~3.1 h | **NO** | 365 MB |
| **kokoro-en-v0_19 (CHOSEN)** | **3.00x** | ~3.3 h | **YES** | 320 MB |

RTF was reproduced twice on multi-lang (3.31x, 3.27x — within 1%), so the number is
repeatable, not a single lucky run. Model load ~2s.

**Verdict: GO.** Kokoro is comfortably faster than realtime on this laptop. No fallback
to Piper as default needed. Cross-chapter rayon parallelism should improve on this further
(`KokoroTts` is `Send + Sync`).

---

## FINDING 1 — Model choice: the rhotic-schwa token trap

**Symptom:** 11x `Skip unknown phonemes. Unicode codepoint: \U+025A` in 135 words, and
audibly clipped r's on "winter", "harbor", "leather", "innkeeper" (confirmed by ear).

**Root cause:** U+025A (ɚ, rhotic schwa) is present in `kokoro-en-v0_19/tokens.txt` but
**absent** from `kokoro-multi-lang-v1_1/tokens.txt` — the multilingual model dropped it,
presumably for Chinese coverage. espeak-ng correctly emits ɚ for American English, the
model has no matching token, sherpa silently discards it.

```
grep -c "ɚ" kokoro-en-v0_19/tokens.txt        # 1
grep -c "ɚ" kokoro-multi-lang-v1_1/tokens.txt # 0
```

**Resolution:** use `kokoro-en-v0_19`. Zero phoneme warnings, verified. Also simpler
config — no `dict_dir`, no `lexicon` files, no Chinese FSTs (pass empty strings).

> **DO NOT "upgrade" to kokoro-multi-lang.** The higher version number is a silent
> quality regression for English. Cost of the correct choice is ~8% RTF (3.3h vs 3.1h
> per 10h book) and loss of non-English support, which this project does not need.

---

## FINDING 2 — Normalization is validated, not assumed (justifies Task 1.5)

Two artifacts heard in the first listen:
1. `1904` read as "nineteen hundred four" (wanted: "nineteen oh four")
2. An unwanted pause after `Dr.` mid-sentence

Cause of (2): sherpa's sentence splitter treats the period in `Dr.` as end-of-sentence,
so Kokoro renders two sentences with an end-of-sentence prosody drop.

A/B harness (`spike/src/main.rs`) synthesizes each case raw vs. normalized. Durations
alone prove the pause removal — normalized is consistently shorter:

| Case | raw | normalized | reclaimed |
|---|---|---|---|
| abbrev (`Dr.`, `1904`) | 4.56 s | 4.03 s | **0.53 s** |
| titles (`Mr. St. Mrs. p.m. St.`) | 5.61 s | 4.60 s | **1.00 s** |
| numbers (`1987 3rd $4.50 12% 1902`) | 6.56 s | 6.38 s | 0.18 s |

`numbers` moves least because most of its periods are genuine sentence-enders — the win
scales with abbreviation density, which is high in real prose with names and dialogue.

**Both reported defects collapse into one work item: Task 1.5 normalization.** Feed
Kokoro "Doctor" and "nineteen oh four" as literal words and there is no period to
mis-split and no number to mis-read. This confirms Task 1.5 is the highest-value module
in the project.

---

## Verification

Ad-hoc script (not a test suite — the spike is throwaway Phase-0 proof code):

```bash
/tmp/hermes-verify-ab.sh
```

13/13 checks passed 2026-08-02: build + `fmt --check` + `clippy -D warnings` clean,
zero phoneme warnings, all 6 WAVs real audio (24kHz, -24..-25 dB mean), and
normalized < raw duration for all three pairs.

### Reproduce

```bash
cd ~/Repos/epub2audiobook-rs/spike && cargo run --release
ffplay -autoexit -nodisp ab-titles-raw.wav    # hear the pauses
ffplay -autoexit -nodisp ab-titles-norm.wav   # hear them gone
```
