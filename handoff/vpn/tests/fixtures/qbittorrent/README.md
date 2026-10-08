# qBittorrent fixtures

- **conf editor (SPEC §5.2):** editing `*.conf` with port 53186 must produce exactly the matching `*.expected-53186.conf`. Only the listed keys change; order, comments and other keys are preserved. Unknown layout → no change + `listen_port_applied = none`. **VERIFY the key names against DC's installed version.**
- **Web UI:**
  - `POST /api/v2/auth/login` returns body `Ok.` (+ `Set-Cookie: SID=…`) or `Fails.`; HTTP 403 after too many failures.
  - `GET /api/v2/app/preferences` returns JSON; the keys used are `listen_port`, `upnp`, `random_port`.
