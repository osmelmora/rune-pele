# Agent notes for rune-pele

- Pele 1.13.0 is the source of truth for syntax, layout and output. Its source sits in `vendor/pele` at `scripts/pele-pin.txt`. Read it there, and never edit it.
- Run every tool through mise (`mise exec -- …` or `mise run <task>`). The Node version decides what the oracle prints, so a bare `node` can make a parity claim false.
- Parity claims come from the oracle, never from hand-written expected output. Before claiming a diagram type, CLI behavior or number helper works, follow `.agents/skills/verify-rune-pele/SKILL.md`.
- Any change to `crates/pele-core/src/js.rs` or the Pele pin goes through `.agents/skills/prove-js-numerics/SKILL.md`.
- Numbers that reach SVG go through `pele_core::js` (`num`, `write_num`, `to_string`, `to_fixed`, `round`). Never use Rust's `f64::round`, `Display` or `{:.N}` for them.
- `mise run check` must pass before a commit.
