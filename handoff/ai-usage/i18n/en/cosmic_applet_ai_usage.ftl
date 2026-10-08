# AI Usage applet — English strings. Keys are referenced from docs/SPEC.md.

applet-name = AI Usage
provider-claude = Claude
applet-settings = Applet settings
back = Back
refresh-now = Refresh now
refreshing = Refreshing…

# Windows
session = Session
session-desc = 5-hour window
weekly = Weekly
weekly-desc = All models · 7 days
fable = Fable
fable-desc = Fable · 7 days
label-session = 5h
label-weekly = Week
label-fable = Fable
label-reset = Reset

# Amounts and pace
used = used
left = left
on-pace = On pace
ahead-of-pace = { $pct }% ahead of pace
under-pace = { $pct }% under pace
limit-reached = Limit reached · { $reset }

# Reset text
resets-in = Resets in { $time }
resets-at = Resets { $time }
resets-tomorrow-at = Resets tomorrow { $time }
resetting-now = Resetting now

# Freshness
updated-just-now = Updated just now
updated-ago = Updated { $time } ago
updated-never = Updated —
offline-ago = Offline · { $time } ago
rate-limited-retry = Rate limited · retry in { $time }

# Banners
signin-title = Sign in with Claude Code
signin-body = Run `claude` in a terminal and log in. This applet reads that login and updates automatically.
login-no-profile = The Claude Code login doesn't include profile access. Log out and in again with `claude`.
expired-title = Claude Code login expired
expired-body = Open Claude Code once to renew it. Showing the last values.
format-title = Usage format not recognised
format-body = Claude may have changed its usage service. Values can't be shown until the applet is updated.
copy-diagnostics = Copy diagnostics
no-fable-limit = No Fable limit on this plan.

# Settings
show-in-panel = Show in panel
session-reset = Session reset
session-reset-desc = Time until the 5-hour window resets
at-least-one = At least one stays on
panel-style = Panel style
style-percent = Percent
style-bars = Bars
style-both = Both
show = Show
amount-used = Used
amount-left = Left
reset-times = Reset times
reset-relative = Relative
reset-absolute = Clock time
refresh-every = Refresh every
minutes = { $n } min

# Accessible name / tooltip
a11y-window = { $name } { $pct }% { $amount }, { $reset }
a11y-signed-out = Claude usage: not signed in

# Panel icon
panel-icon = Panel icon
icon-robot = Robot
icon-cyborg-cyan = Cyborg · cyan
icon-cyborg-red = Cyborg · red
icon-note-symbolic = Symbolic · follows the theme
icon-note-avatar = Full colour · ring shows session left
ring-left = Session { $pct }% left
ring-limit = Session limit reached
ring-no-data = Session: no data
