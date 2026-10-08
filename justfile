# Yutani system monitoring: three COSMIC panel applets.

prefix := env_var_or_default("PREFIX", "/usr/local")
bindir := prefix / "bin"
appdir := prefix / "share/applications"
icondir := prefix / "share/icons/hicolor/scalable"

applets := "net-traffic sysmon ai-usage"

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
# The optional RAPL udev rule is never installed; see the README.
install: build
    install -Dm0755 target/release/cosmic-applet-net-traffic {{bindir}}/cosmic-applet-net-traffic
    install -Dm0755 target/release/cosmic-applet-sysmon {{bindir}}/cosmic-applet-sysmon
    install -Dm0755 target/release/cosmic-applet-ai-usage {{bindir}}/cosmic-applet-ai-usage
    install -Dm0644 crates/ai-usage/resources/io.github.dc.CosmicAppletAiUsage.desktop {{appdir}}/io.github.dc.CosmicAppletAiUsage.desktop
    install -Dm0644 crates/ai-usage/resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletAiUsage-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletAiUsage-symbolic.svg
    install -Dm0644 crates/net-traffic/resources/io.github.dc.CosmicAppletNetTraffic.desktop {{appdir}}/io.github.dc.CosmicAppletNetTraffic.desktop
    install -Dm0644 crates/sysmon/resources/io.github.dc.CosmicAppletSysMon.desktop {{appdir}}/io.github.dc.CosmicAppletSysMon.desktop
    install -Dm0644 crates/net-traffic/resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletNetTraffic-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletNetTraffic-symbolic.svg
    install -Dm0644 crates/net-traffic/resources/icons/hicolor/scalable/actions/net-down-bar-symbolic.svg {{icondir}}/actions/net-down-bar-symbolic.svg
    install -Dm0644 crates/net-traffic/resources/icons/hicolor/scalable/actions/net-up-bar-symbolic.svg {{icondir}}/actions/net-up-bar-symbolic.svg
    install -Dm0644 crates/sysmon/resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletSysMon-symbolic.svg {{icondir}}/apps/io.github.dc.CosmicAppletSysMon-symbolic.svg

uninstall:
    rm -f {{bindir}}/cosmic-applet-net-traffic {{bindir}}/cosmic-applet-sysmon {{bindir}}/cosmic-applet-ai-usage
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
demo:
    AI_USAGE_DEMO=1 APPLET_PREVIEW=1 cargo run -p cosmic-applet-ai-usage --features demo

# The popup in an ordinary window, for visual checks without a panel.
preview applet="sysmon":
    APPLET_PREVIEW=1 cargo run -p cosmic-applet-{{applet}}
