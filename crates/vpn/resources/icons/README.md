# Reticle icons (custom)

| File | State |
|---|---|
| `vpn-reticle-off-symbolic.svg` | No tunnel up. Brackets open, both » are thin lines. |
| `vpn-reticle-p2p-symbolic.svg` | Torrent tunnel up. First » solid. |
| `vpn-reticle-web-symbolic.svg` | Web tunnel up. Second » solid. |
| `vpn-reticle-both-symbolic.svg` | Both up. Both » solid, brackets 1px in. |

- **Browser tunnel:** not a file. While it's up, the applet adds a short bar on the bottom edge of the frame to whichever of the four it's showing (`widgets/reticle.rs`).
- **Format:** 16 × 16 grid, single ink (#222222, recoloured as symbolic).
- **Install** to `$PREFIX/share/icons/hicolor/scalable/apps/`.
- **Origin:** original artwork drawn for this applet, the same licence as the code.
