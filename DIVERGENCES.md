# Divergences from Pele in product mode

Parity mode (`pele_core::render`, `pele parity`) is byte-identical to Pele 1.13.0 and has no divergences.

Product mode (`rune_pele::Diagram::view`) draws the same Scene natively in Rune. Each deliberate difference from Pele's look is listed here with its reason. A difference that is not listed is a bug.

| Area | Pele | rune-pele product mode | Why |
| --- | --- | --- | --- |
| Label ink on series colours | Contrasting ink only for series a source overrides. Otherwise labels inherit the background colour, so a 17% label on orange is white. | Ink contrasts with the resolved series colour for every series. | Readability on any palette, including app themes Pele never sees. |
| Fonts | Arial metrics in parity mode, the browser's font in a page. | The app's Rune fonts, measured by Rune at device scale. | Labels fit the font that is actually drawn. |
| Bold weight | `font-weight: bold` (700). | Weight 700 requested. Rune's embedded Inter tops out at SemiBold. | Rune's font set. Unverified on other fonts. |
| Sub/superscript shift | `baseline-shift` sub/super. | Smaller runs shifted by 0.3 and −0.45 of the smaller size. | Rune has no baseline-shift. The values are guesses until a type with sub/sup is checked side by side. |
