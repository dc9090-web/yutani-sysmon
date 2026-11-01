# AI Usage applet — English strings. Keys are referenced from docs/SPEC.md.

applet-name = AI Usage
provider-claude = Claude
applet-settings = Applet settings
back = Back
refresh-now = Refresh now
refreshing = Refreshing…
not-signed-in = Not signed in

# Windows
weekly = Session
weekly-desc = All models · 7 days
fable = Fable
fable-desc = Fable · 7 days
label-weekly = Week
label-fable = Fable
label-reset = Reset
label-reset-now = now
label-reset-none = —
label-credits = API

# API credits (Claude Platform)
credits = API credits
credits-desc = { $limit } a month · Claude API
credits-of = of { $limit }
credits-used-up = Credits used up · { $renew }
renews-in = Renews in { $time }
renews-on = Renews { $date }
renewing-now = Renewing now
credits-rejected = Admin key not accepted
credits-offline = API credits: couldn't update
credits-format = API credits: spend format not recognised
credits-loading = Checking API credits…

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
toggle-weekly-desc = All models · 7 days
toggle-fable-desc = Fable weekly limit
toggle-credits-desc = This month's API spend against the credit
weekly-reset = Weekly reset
weekly-reset-desc = Time until the 7-day window resets
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

# API credits settings
api-credits = API credits
admin-key-placeholder = Paste your Admin API key
admin-key-placeholder-saved = Key saved · paste a new one to replace it
admin-key-shape = Anthropic keys start with sk-ant-
test = Test
remove = Remove
checking = Checking…
key-works = Key works
key-invalid = Key not accepted · it must be an Admin key
key-offline = Couldn't check: offline
key-rate-limited = Couldn't check: rate limited
key-format = Couldn't read the cost report
key-no-keyring = No system keyring found · the key wasn't saved
admin-key-tip = Create one at platform.claude.com/settings/admin-keys. Spend lags Anthropic by a few minutes.
monthly-credit = Monthly credit
credit-amount = ${ $n }
renew-day = Renews on day

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
a11y-credits = API credits { $spent } of { $limit } used, { $renew }

# Panel icon
panel-icon = Panel icon
icon-robot = Robot
icon-cyborg-cyan = Cyborg · cyan
icon-cyborg-red = Cyborg · red
icon-note-symbolic = Symbolic · follows the theme
icon-note-avatar = Full colour · ring shows weekly left
ring-left = Week { $pct }% left
ring-limit = Weekly limit reached
ring-no-data = Weekly: no data
