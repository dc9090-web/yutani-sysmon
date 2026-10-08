# Fixtures

All keys here are **dummies** (`AAAA…=`, `BBBB…=`). No real credentials are in this repo.

| Folder | Used by |
|---|---|
| `conf/` + `expected.json` | `common::conf` parser tests (SPEC §3.2), including rejected files |
| `natpmp/` | `common::natpmp` codec tests (SPEC §4.3) |
| `wg/` | Status mapping: handshake age, stale, timeout (SPEC §4.1, §4.4) |
| `qbittorrent/` | Conf-file port editor and Web UI client (SPEC §5.2, §5.4) |

`just demo` uses these to drive a fake helper and a fake NetworkManager.
