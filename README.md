# reezy

Convert EPUB ebooks into chapter-tagged M4B audiobooks using local neural TTS.

Runs once, produces a file, exits. No server, no daemon, nothing running while you
listen. The resulting `.m4b` plays offline on an iPhone, Android, or anything else
that reads audiobooks.

## Status

Early. Phase 0 (feasibility) is complete and validated; Phase 1 (EPUB parsing and
text normalization) is in progress. See `.hermes/plans/` for the implementation plan
and `BENCHMARKS.md` for measured performance and findings.

## How it works

```
EPUB -> chapters -> normalized text -> Kokoro TTS -> M4B + chapter markers
```

Text-to-speech runs locally via [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
with the [Kokoro-82M](https://github.com/hexgrad/kokoro) model. Measured at **3x
faster than realtime** on an AMD Ryzen 5 PRO 7530U (CPU only, no GPU) -- a 10-hour
audiobook renders in roughly 3 hours.

## Requirements

- Rust toolchain
- ffmpeg
- Kokoro model: `kokoro-en-v0_19` from the sherpa-onnx releases page, extracted to
  `~/.local/share/abook/models/kokoro-en-v0_19/`

> **Use `kokoro-en-v0_19`, not `kokoro-multi-lang`.** The multilingual model omits the
> rhotic schwa (U+025A) from its token set, which audibly clips the r in words like
> "winter" and "harbor". See BENCHMARKS.md Finding 1.

## Usage

```bash
reezy extract book.epub -o ./out/
```

More subcommands land as the phases complete.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## License

MIT
