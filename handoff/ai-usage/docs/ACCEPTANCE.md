# Acceptance checklist

Run every check in **dark and light**, at panel sizes XS, S and M, on a horizontal top panel and a vertical dock. Compare with `design/screenshots/`.

## Auth and safety

- [ ] S-01: With Claude Code logged in, the applet shows data with zero setup.
- [ ] S-02: **The credentials files are byte-identical and keep their mtime** before and after 24 h of running (`sha256sum` and `stat`). The applet never calls the token endpoint (check with `strace -f -e trace=network` or an HTTPS proxy log: the only host is `api.anthropic.com`, path `/api/oauth/usage`).
- [ ] S-03: Each fake home in `tests/fixtures/claude-home/` produces its documented state. Pointing `CLAUDE_CONFIG_DIR` at one works.
- [ ] S-04: After `claude` renews an expired login, the applet recovers within 30 s, with no restart.
- [ ] S-05: `RUST_LOG=trace` output contains no `sk-ant-` string. Neither does "Copy diagnostics".
- [ ] S-06: Nothing is written to disk except the cosmic-config settings directory.

## Data

- [ ] D-01: Session, Weekly and Fable values match claude.ai → Settings → Usage within 1 point after a refresh.
- [ ] D-02: Every fixture in `tests/fixtures/usage/` maps to its documented state. `unknown_shape.json` shows "Usage format not recognised" with no panic.
- [ ] D-03: A plan with no Fable limit hides the Fable row and chunk, and disables its toggler.
- [ ] D-04: A 429 with `Retry-After: 120` pauses polling for 120 s, and the header shows the countdown.
- [ ] D-05: Offline (unplug the network): stale dimming within one interval, and recovery after reconnecting with no restart.
- [ ] D-06: When a window's `resets_at` passes, it shows 0% / "Resetting now" and refetches within 60 s.
- [ ] D-07: Idle CPU is below 0.2% of one core. With the default 5-minute interval, there are 12 ± 2 requests per hour.

## Panel

- [ ] P-01: The default is the robot plus Bars with Session, Weekly and Fable (`panel-*.png`).
- [ ] P-02: The robot is always first, at the panel icon size, and never tinted.
- [ ] P-03: The width stays constant while values change (0 → 100%, countdown 9m → 3d4h).
- [ ] P-04: The fill is the accent below 80%, warning at 80–99%, and destructive at 100% with no pace tick. Change the COSMIC accent: the fill follows it live.
- [ ] P-05: Percent, Bars and Both, with Used and Left, and the Reset chunk, all render as in the screenshots.
- [ ] P-06: The tooltip lists every enabled window with its % and reset, plus the state suffix.

## Popup

- [ ] U-01: Header: robot, "Claude", plan chip, email, freshness and refresh. The refresh button disables while fetching and for 10 s afterwards.
- [ ] U-02: Rows: the pace tick sits at the expected % (check against a hand calculation); the reset text follows the Relative/Clock time setting and the 12/24-hour preference.
- [ ] U-03: Every state in SPEC §9 matches `states-*.png`.
- [ ] U-04: The settings page applies every change live and persists across a panel restart. At least one window toggle stays on.

## Code

- [ ] C-01: `just check` and `just test` pass. `format.rs`, `api.rs` and `auth.rs` have fixture-driven tests.
- [ ] C-02: There are no hex literals; colours come from `theme.cosmic()`.
- [ ] C-03: Every visible string is in the `.ftl` file.
- [ ] C-04: `just demo` cycles through every state without the network.
