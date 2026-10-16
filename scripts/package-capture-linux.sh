#!/usr/bin/env bash
# Package MiSTerKVMS Capture for Debian/Ubuntu (x86_64) from a release build:
#   build/misterkvms-capture-<version>-linux-amd64.deb  (the program, the systemd
#       unit, the misterkvms user; starts the service on install)
# The Linux app tarball (scripts/build-linux.sh) carries the program too, for
# other distributions: `sudo ./misterkvms-capture service install` there.
# Usage: scripts/package-capture-linux.sh [version]   (after
#   cargo build --release -p misterkvms-capture; needs dpkg-deb)
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}
BIN="$ROOT/target/release/misterkvms-capture"
OUT="$ROOT/build"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
[[ -x "$BIN" ]] || { echo "build it first: cargo build --release -p misterkvms-capture" >&2; exit 1; }
mkdir -p "$OUT"

echo "==> .deb"
D="$WORK/deb"
mkdir -p "$D/DEBIAN" "$D/usr/bin" "$D/lib/systemd/system" "$D/usr/share/doc/misterkvms-capture"
install -m 755 "$BIN" "$D/usr/bin/misterkvms-capture"
"$BIN" systemd-unit /usr/bin/misterkvms-capture > "$D/lib/systemd/system/misterkvms-capture.service"
cp "$ROOT/LICENSE" "$D/usr/share/doc/misterkvms-capture/copyright"
SIZE=$(du -sk "$D/usr" "$D/lib" | awk '{s += $1} END {print s}')
cat > "$D/DEBIAN/control" <<EOF
Package: misterkvms-capture
Version: $VERSION
Section: video
Priority: optional
Architecture: amd64
Depends: libasound2t64 | libasound2, systemd
Installed-Size: $SIZE
Maintainer: MiSTerKVMS <https://github.com/danifunker/MiSTerKVMS>
Homepage: https://github.com/danifunker/MiSTerKVMS
Description: MiSTerKVMS Capture: capture card streams for MiSTerKVMS
 Streams the capture cards that show MiSTer FPGAs to MiSTerKVMS viewers
 (Mac, Windows, Linux, iPhone), and passes their keyboard and mouse on to the
 MiSTer. Runs as a systemd service as its own user (misterkvms); set it up at
 http://localhost:5971.
EOF
cat > "$D/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    if ! getent passwd misterkvms >/dev/null; then
        useradd --system --user-group --home-dir /var/lib/misterkvms-capture --no-create-home \
            --shell /usr/sbin/nologin misterkvms
    fi
    for g in video audio; do
        if getent group "$g" >/dev/null; then usermod -aG "$g" misterkvms; fi
    done
    mkdir -p /var/lib/misterkvms-capture
    chown -R misterkvms:misterkvms /var/lib/misterkvms-capture
    chmod 750 /var/lib/misterkvms-capture
    if [ -d /run/systemd/system ]; then
        systemctl daemon-reload
        systemctl enable misterkvms-capture.service >/dev/null
        # A new install starts it; an upgrade restarts it on the new program.
        systemctl restart misterkvms-capture.service
        echo "MiSTerKVMS Capture is running: set it up at http://localhost:5971"
    fi
fi
exit 0
EOF
cat > "$D/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ] && [ -d /run/systemd/system ]; then
    systemctl disable --now misterkvms-capture.service >/dev/null 2>&1 || true
fi
exit 0
EOF
cat > "$D/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload || true
fi
if [ "$1" = purge ]; then
    rm -rf /var/lib/misterkvms-capture
    if getent passwd misterkvms >/dev/null; then userdel misterkvms || true; fi
fi
exit 0
EOF
chmod 755 "$D/DEBIAN/postinst" "$D/DEBIAN/prerm" "$D/DEBIAN/postrm"
dpkg-deb --root-owner-group --build "$D" "$OUT/misterkvms-capture-$VERSION-linux-amd64.deb" >/dev/null

echo "Built:"
ls -1 "$OUT"/misterkvms-capture-"$VERSION"-linux-*
