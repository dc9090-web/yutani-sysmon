# AI Usage applet — English strings. Keys are referenced from docs/SPEC.md.

applet-name = AI Usage
provider-claude = Claude
applet-settings = Applet settings
back = Back
refresh-now = Refresh now
refreshing = Refreshing…
not-signed-in = Not signed in

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
label-reset-now = now
label-reset-none = —

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
offline = Offline
offline-just-now = Offline · just now

# Banners
signin-title = Sign in to Claude
signin-body = Sign in with your Claude account to see your usage limits. The applet keeps itself signed in. If you use Claude Code on this computer, its login works too.
login-no-profile = The Claude Code login doesn't include profile access. Sign in here instead, or log out and in again with `claude`.
expired-title = Claude Code login expired
expired-body = Sign in here and the applet stays signed in on its own, or open Claude Code to renew its login. Showing the last values.
expired-own-title = Claude sign-in expired
expired-own-body = Sign in again to keep your usage up to date. Showing the last values.
signin-waiting-title = Finish signing in in your browser
signin-waiting-body = Approve the request on claude.ai. This updates as soon as you're done.
signin-failed-title = Sign-in didn't finish
signin-failed-timeout = The browser didn't come back in time.
signin-failed-denied = The request was declined on claude.ai.
signin-failed-local = Couldn't open the browser or wait for its reply.
signin-failed-exchange = Claude didn't accept the sign-in. Try again in a moment.
sign-in = Sign in
sign-out = Sign out
cancel = Cancel
try-again = Try again
format-title = Usage format not recognised
format-body = Claude may have changed its usage service. Values can't be shown until the applet is updated.
copy-diagnostics = Copy diagnostics
no-fable-limit = No Fable limit on this plan.

# Settings
account = Account
account-own = Signed in to Claude
account-own-desc = Renews itself
account-claude-code = Using Claude Code's login
account-claude-code-desc = Sign in here to keep working when Claude Code isn't run on this computer.
show-in-panel = Show in panel
toggle-weekly-desc = All models
toggle-fable-desc = Fable weekly limit
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
a11y-window-no-reset = { $name } { $pct }% { $amount }
a11y-signed-out = Claude usage: not signed in
a11y-loading = Claude usage: loading
a11y-format = Claude usage: format not recognised
a11y-offline = offline, updated { $time } ago
a11y-offline-never = offline
a11y-rate-limited = rate limited, retry in { $time }
a11y-expired = Claude login expired

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
