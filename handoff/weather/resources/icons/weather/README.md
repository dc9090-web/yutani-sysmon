# Detailed weather icons (Pixeden): embed only

These 30 files are converted from the Pixeden "Weather App Icons" pack that DC supplied: 24 weather conditions plus `temp-high`, `temp-mid`, `temp-low`, `humidity`, `compass` and `compass-alt`. They're single-ink `#232323` symbolic SVGs on a 16 × 16 grid.

**Licence:** `LICENSE-PIXEDEN.txt` (the 2012 copy from the pack).
- Royalty-free for personal and commercial use, modifiable, no attribution required.
- **No redistribution** of the resources themselves.

**Rules for the build:**
1. **Embed them in the binary** with `include_bytes!` (see `src/icons.rs` in SPEC §11). Never install them to `share/icons`, and never ship them as loose files in a package.
2. Load each one with `icon::from_svg_bytes(BYTES).symbolic(true)`.
3. **Public source release is blocked** until DC confirms with Pixeden, because this folder would be published in the repo. Until then, keep the repo private, or move this folder out of the public tree (e.g. a git-ignored directory with a `build.rs` check) and default the setting to `System`.
