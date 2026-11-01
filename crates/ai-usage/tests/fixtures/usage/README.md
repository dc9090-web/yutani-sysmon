These are sample usage-endpoint responses for parser tests. The field names and shapes mirror the fields the open-source YapCap applet parses from `GET https://api.anthropic.com/api/oauth/usage` (verified against its source, Sep 2026).

The endpoint is undocumented, so treat these files as the contract the parser must accept. Anything else (e.g. `unknown_shape.json`) maps to the "format not recognised" state.

The timestamps assume "now" = `2026-10-08T10:00:00Z`.

`weekly_only.json` (added 2026-11-01) mirrors a live response after Claude dropped the 5-hour window from a plan: `five_hour` is null, the Fable entry's model `id` is null (matched by `display_name`), and the extra `spend`, `extra_usage` and `seven_day_breakdown` blocks are ignored.
