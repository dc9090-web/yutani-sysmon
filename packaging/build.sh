#!/bin/sh
# Builds .deb, .rpm and Arch packages for every applet into dist/, from an
# existing `cargo build --release`. Needs nfpm on PATH.
# Usage: packaging/build.sh <version>   (e.g. 0.2.0)
set -eu
cd "$(dirname "$0")/.."
VERSION=${1:?version, e.g. 0.2.0}
export VERSION
rm -rf dist && mkdir dist
for applet in net-traffic sysmon ai-usage weather vpn; do
    for format in deb rpm archlinux; do
        nfpm package --config "packaging/$applet.yaml" --packager "$format" --target dist/
    done
done
(cd dist && sha256sum -- * > SHA256SUMS)
ls -l dist
