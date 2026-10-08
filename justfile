# Yutani system monitoring: five COSMIC panel applets.

prefix := env_var_or_default("PREFIX", "/usr/local")
bindir := prefix / "bin"
appdir := prefix / "share/applications"
icondir := prefix / "share/icons/hicolor/scalable"

applets := "net-traffic sysmon ai-usage weather vpn"

# Release build of all applets.
build:
    cargo build --release

# Lints and formatting.
check:
    cargo clippy --all-targets -- -D warnings
    cargo fmt --check

test:
    cargo test --workspace

# Install binaries, desktop entries and icons under $PREFIX (default /usr/local).
# The optional RAPL udev rule is never installed; see the README. Weather
# installs no icons: its Detailed set is embedded and its list icon is a system one.
install: build
    install -Dm0755 target/release/cosmic-applet-net-traffic {{bindir}}/cosmic-applet-net-traffic
    install -Dm0755 target/release/cosmic-applet-sysmon {{bindir}}/cosmic-applet-sysmon
    install -Dm0755 target/release/cosmic-applet-ai-usage {{bindir}}/cosmic-applet-ai-usage
    install -Dm0755 target/release/cosmic-applet-weather {{bindir}}/cosmic-applet-weather
    install -Dm0755 target/release/cosmic-applet-vpn {{bindir}}/cosmic-applet-vpn
    install -Dm0644 crates/vpn/resources/io.github.dc.CosmicAppletVpn.desktop {{appdir}}/io.github.dc.CosmicAppletVpn.desktop
    for i in off p2p web both; do install -Dm0644 crates/vpn/resources/icons/vpn-reticle-$i-symbolic.svg {{icondir}}/apps/vpn-reticle-$i-symbolic.svg; done
    install -Dm0644 crates/weather/resources/io.github.dc.CosmicAppletWeather.desktop {{appdir}}/io.github.dc.CosmicAppletWeather.desktop
    install -Dm0644 crates/ai-usage/resources/io.github.dc.CosmicAppletAiUsage.desktop {{appdir}}/io.github.dc.CosmicAppletAiUsage.desktop
    install -Dm0644 crates/ai-usage/resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletAiUsage-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletAiUsage-symbolic.svg
    install -Dm0644 crates/net-traffic/resources/io.github.dc.CosmicAppletNetTraffic.desktop {{appdir}}/io.github.dc.CosmicAppletNetTraffic.desktop
    install -Dm0644 crates/sysmon/resources/io.github.dc.CosmicAppletSysMon.desktop {{appdir}}/io.github.dc.CosmicAppletSysMon.desktop
    install -Dm0644 crates/net-traffic/resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletNetTraffic-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletNetTraffic-symbolic.svg
    install -Dm0644 crates/net-traffic/resources/icons/hicolor/scalable/actions/net-down-bar-symbolic.svg {{icondir}}/actions/net-down-bar-symbolic.svg
    install -Dm0644 crates/net-traffic/resources/icons/hicolor/scalable/actions/net-up-bar-symbolic.svg {{icondir}}/actions/net-up-bar-symbolic.svg
    install -Dm0644 crates/sysmon/resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletSysMon-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletSysMon-symbolic.svg

uninstall:
    rm -f {{bindir}}/cosmic-applet-net-traffic {{bindir}}/cosmic-applet-sysmon {{bindir}}/cosmic-applet-ai-usage {{bindir}}/cosmic-applet-weather
    rm -f {{appdir}}/io.github.dc.CosmicAppletWeather.desktop
    rm -f {{bindir}}/cosmic-applet-vpn {{appdir}}/io.github.dc.CosmicAppletVpn.desktop
    for i in off p2p web both; do rm -f {{icondir}}/apps/vpn-reticle-$i-symbolic.svg; done
    rm -f {{appdir}}/io.github.dc.CosmicAppletAiUsage.desktop {{icondir}}/apps/io.github.dc.CosmicAppletAiUsage-symbolic.svg
    rm -f {{appdir}}/io.github.dc.CosmicAppletNetTraffic.desktop {{appdir}}/io.github.dc.CosmicAppletSysMon.desktop
    rm -f {{icondir}}/apps/io.github.dc.CosmicAppletNetTraffic-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletSysMon-symbolic.svg
    rm -f {{icondir}}/actions/net-down-bar-symbolic.svg {{icondir}}/actions/net-up-bar-symbolic.svg

# Applets only render inside the panel: install, then add them in
# Settings → Desktop → Panel → Applets. This runs one with debug logs.
run applet="sysmon":
    RUST_LOG=cosmic_applet_{{replace(applet, "-", "_")}}=debug cargo run -p cosmic-applet-{{applet}}

# AI Usage's popup in a window, cycling through every state every 10 s from
# the test fixtures: no login, no network. AI_USAGE_DEMO_SCENE=n starts on scene n.
# AI_USAGE_DEMO_ICON=cyan|red shows an avatar without saving it.
# AI_USAGE_SHOT=file.pam saves the window as an image and exits (README screenshots).
demo:
    AI_USAGE_DEMO=1 APPLET_PREVIEW=1 cargo run -p cosmic-applet-ai-usage --features demo

# The popup in an ordinary window, for visual checks without a panel.
preview applet="sysmon":
    APPLET_PREVIEW=1 cargo run -p cosmic-applet-{{applet}}

# Weather's popup in a window, cycling through every state every 10 s from the
# test fixtures: no key, no network. WEATHER_DEMO_SCENE=n starts on scene n;
# WEATHER_SHOT=file.pam saves the window as an image and exits.
weather-demo:
    WEATHER_DEMO=1 APPLET_PREVIEW=1 cargo run -p cosmic-applet-weather --features demo

# Weather: one real call per OpenWeather endpoint with the stored key; prints
# each response's key paths that differ from tests/fixtures (never values).
live-check:
    cargo test -p cosmic-applet-weather live_check -- --ignored --nocapture

# VPN: the root helper and its D-Bus, systemd and polkit files (system-wide,
# needs sudo). `just vpn-helper-uninstall` also removes a tunnel left up.
vpn-helper-install:
    cargo build --release -p cosmic-vpn-helper
    sudo crates/vpn-helper/install.sh

vpn-helper-uninstall:
    sudo crates/vpn-helper/install.sh --uninstall

# VPN's popup in a window, cycling through every state every 8 s with a fake
# helper and NetworkManager. VPN_DEMO_SCENE=n starts on scene n.
vpn-demo:
    VPN_DEMO=1 APPLET_PREVIEW=1 cargo run -p cosmic-applet-vpn --features demo

# VPN: what the parser makes of a Proton conf, never showing its key.
vpn-check-conf +files:
    cargo run -q -p cosmic-applet-vpn -- --check-conf {{files}}

# VPN: live leak and kill-switch checks against a running torrent tunnel (sudo).
vpn-acceptance:
    sudo crates/vpn-helper/acceptance.sh
