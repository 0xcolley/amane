# amane — 0xcolley's copy

This is a personal copy of **[amane](https://github.com/MystiaFin/amane)**, the Rust library for building Wayland desktop shells, made by **[MystiaFin](https://github.com/MystiaFin)**. All credit for amane goes to them. To read about amane itself, see the original [README](../README.md) or the [documentation](https://mystiafin.github.io/amane/).

## Why it isn't a fork

This repo is deliberately not a GitHub fork:

- **Self-use only.** It exists for my own desktop shell while amane is on hiatus. Nothing here is meant to go back upstream, and nothing is submitted to the original repository: no pull requests, no issues.
- **No collisions with future changes.** It isn't linked to the original, so nothing here can be mistaken for, or end up in, the original project, and pull requests can't default to it. My changes sit as separate commits on top of upstream, so upstream's own future work can still be pulled in cleanly when amane returns.

This file lives in `.github/` so that the original `README.md` stays exactly as upstream has it.

## Changes from upstream

Based on MystiaFin/amane at `544cbfc`.

- **`Rectangle::backdrop()`:** asks the compositor to blur what's behind a shape, through `ext-background-effect-v1`. The blur follows the shape exactly, including rounded corners. This needs a compositor that supports the protocol, such as niri 26.04 or later.
- **`Path::svg()` and `Path::view_box()`:** draw SVG path data, such as an icon's `d` attribute, in a `Canvas`. `view_box(x, y, width, height)` fits the data's own coordinates into the canvas, keeping its shape and centring it, like SVG's `meet`.
- **Text rendering:**
  - **Shaping:** text is shaped with swash, so a font's kerning and ligatures apply.
  - **Letter placement:** letters are placed to a quarter pixel instead of each snapping to a whole pixel, which evens out the gaps between them.
  - **Light text:** the soft edges of light letters are lifted, so white text on dark backgrounds no longer reads thin.
  - The same applies to every widget that measures or draws text, so wrapping, eliding and the text-input caret follow the shaped widths.

## Support

This copy isn't maintained for anyone else. Please don't report problems with it to the original repository; they aren't the original developer's to fix.

## License

MIT, © 2026 MystiaFin. See [LICENSE](../LICENSE). The changes here are under the same license.
