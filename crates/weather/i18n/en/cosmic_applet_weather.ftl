# Weather applet — English strings. Keys are referenced from docs/SPEC.md.

applet-name = Weather
applet-settings = Applet settings
back = Back
refresh-now = Refresh now
refreshing = Refreshing…
attribution = Weather data: OpenWeather

# Popup
feels-like = Feels like { $temp }
high = H { $temp }
low = L { $temp }
next-24h = Next 24 hours
next-24h-free = Next 24 hours · 3-hour steps
forecast-7 = 7-day forecast
forecast-5 = 5-day forecast
now = Now
today = Today
wind = Wind
humidity = Humidity
uv-index = UV index
sunrise = Sunrise
sunset = Sunset
pressure = Pressure
uv-low = Low
uv-moderate = Moderate
uv-high = High
uv-very-high = Very high
uv-extreme = Extreme
in-panel = in panel
panel-tag = panel

# Freshness
updated-ago = Updated { $time } ago
updated-just-now = Updated just now
offline-ago = Offline · { $time } ago
rate-limited = Rate limited · retry in { $time }
daily-limit = Daily call limit reached · resumes { $time }

# Alerts
alert-meta = { $sender } · { $start }–{ $end }

# Settings
locations = Locations
saved-count = { $n } of 5 saved
saved-full = 5 of 5 saved · remove one to add another
add-city = Add a city
no-matches = No matches
search-unavailable = Search unavailable (check API key)
already-saved = Already saved
remove-location = Remove { $name }
show-in-panel-radio = Show { $name } in panel
weather-icons = Weather icons
icons-detailed = Detailed
icons-system = System
icons-detailed-note = Pixeden weather set · 24 conditions incl. drizzle, heavy rain, wind
icons-system-note = COSMIC icon theme · follows your system icons
units = Units
units-metric = Metric · °C, km/h
units-imperial = Imperial · °F, mph
panel = Panel
show-city = Show city name
show-city-desc = Above the temperature
show-hilo = Show high / low
show-hilo-desc = Today's forecast
refresh-every = Refresh every
minutes = { $n } min
api-key = OpenWeather API key
test = Test
key-placeholder = Paste your API key
key-placeholder-saved = Key saved · paste a new one to replace it
key-shape = A key is usually 32 characters, 0–9 and a–f
checking = Checking…
key-full = Key works · One Call 4.0
key-free = Key works · free plan (limited)
key-invalid = Key not accepted
key-offline = Couldn't check: offline
key-stored-file = No system keyring found · key stored in a private file
key-limit-tip = Set your OpenWeather daily limit to 1,000 in billing to stay free.

# States
no-key-title = Add an OpenWeather API key
no-key-body = Create a free key at openweathermap.org → API keys, then paste it in Applet settings. New keys can take up to 2 hours to activate.
invalid-key-title = API key not accepted
invalid-key-body = OpenWeather returned 401. Check the key, or wait for a new key to activate.
no-locations-title = Choose a location
no-locations-body = Search for a city to start.
open-settings = Open settings
add-location = Add location
free-plan-note = Free plan: 5 days, 3-hour steps, no UV or alerts. Subscribe to One Call for the full forecast.

# Accessible name / tooltip
a11y-panel = { $city }: { $temp }, { $desc } · H { $high } L { $low }
a11y-hourly = Next 24 hours: { $min } to { $max }, rain chance up to { $pop }% at { $time }
a11y-day = { $day }, { $desc }, { $pop }% chance of rain, { $low } to { $high }
