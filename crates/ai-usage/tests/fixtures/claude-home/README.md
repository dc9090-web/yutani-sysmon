These are fake home directories for credential-discovery tests. Point `HOME` (or `CLAUDE_CONFIG_DIR`) at one of them. Every token is a placeholder.

| Folder | Expected state (with "now" = 2026-10-08T10:00:00Z = 1791453600000 ms) |
|---|---|
| `valid/` | Signed in. Plan "Max", email `dc@example.com`. |
| `expired/` | Login expired (`expiresAt` is in the past). |
| `missing-scope/` | Not usable: no `user:profile` scope. Show the "Sign in with Claude Code" banner with the note that the login lacks profile access. |
| (no folder) | Not signed in. |
