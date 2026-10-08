# AI Usage applet: design and spec package

This is the handoff for building a COSMIC panel applet that shows Claude subscription usage: the **Session** (5-hour), **Weekly** and **Fable** limits, with reset times and pace. It reuses the Claude Code login already on the machine, read-only.

## Using it with Claude Code

1. Copy this folder into a new, empty repo.
2. Start Claude Code in it. It reads `CLAUDE.md` automatically.
3. Prompt: *"Read CLAUDE.md and the docs, then do milestone 1."* Continue one milestone at a time.

## Contents

| Path | What it is |
|---|---|
| `CLAUDE.md` | Stack, the read-only auth rules, milestones, open decisions |
| `docs/SPEC.md` | Credentials discovery, usage endpoint and parsing, refresh and backoff, formatting, panel, popup, settings, states, privacy |
| `docs/DESIGN.md` | Colours mapped to libcosmic, robot icon spec, type, geometry, meter rules |
| `docs/ACCEPTANCE.md` | Sign-off checklist, including the "credentials untouched" check |
| `tests/fixtures/usage/` | Sample endpoint responses (normal, no Fable, legacy, limit reached, unknown shape) |
| `tests/fixtures/claude-home/` | Fake Claude Code homes (valid, expired, missing scope) with placeholder tokens |
| `design/prototype/` | Interactive HTML reference. Open `index.html`; the top bar switches pages and theme. |
| `design/screenshots/` | Target renders, dark and light, at 2× |
| `design/icons/` | Robot icon and the cosmic-icons used (CC BY-SA 4.0) |
| `resources/` | Desktop entry and applet icon (the robot) |
| `i18n/en/*.ftl` | Every user-visible string |

## Caveats

- **Undocumented endpoint.** Usage comes from `api.anthropic.com/api/oauth/usage`, the same endpoint Claude Code and YapCap use. It isn't a public API and may change. The applet isolates it and shows "Usage format not recognised" instead of breaking.
- **Read-only login.** The applet never refreshes Claude Code's token. Refreshing would replace Claude Code's saved token and could log Claude Code out. When the login expires, open Claude Code once.

## Where the values come from

The design values come from libcosmic `main` at commit 60ad2cc (2026-10-06). The endpoint fields are taken from YapCap's Claude provider (TopiCsarno/yapcap @ b9beb6f, 2026-09-13).
