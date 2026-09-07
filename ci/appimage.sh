#!/usr/bin/env bash
set -x
rm -rf AppDir *.AppImage *.zsync
set -e

mkdir AppDir

install -Dsm755 target/release/wezterm-mux-server AppDir/usr/bin/sideterm-mux-server
install -Dsm755 target/release/wezterm AppDir/usr/bin/sideterm
install -Dsm755 target/release/wezterm-gui AppDir/usr/bin/sideterm-gui
install -Dsm755 -t AppDir/usr/bin target/release/strip-ansi-escapes
install -Dm644 assets/icon/terminal.png AppDir/usr/share/icons/hicolor/128x128/apps/org.sideterm.sideterm.png
install -Dm644 assets/sideterm.desktop AppDir/usr/share/applications/org.sideterm.sideterm.desktop
install -Dm644 assets/sideterm.appdata.xml AppDir/usr/share/metainfo/org.sideterm.sideterm.appdata.xml
install -Dm644 assets/sideterm-nautilus.py AppDir/usr/share/nautilus-python/extensions/sideterm-nautilus.py

[ -x /tmp/linuxdeploy ] || ( curl -L 'https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage' -o /tmp/linuxdeploy && chmod +x /tmp/linuxdeploy )

TAG_NAME=${TAG_NAME:-$(bash ci/tag-name.sh)}
distro=$(lsb_release -is 2>/dev/null || sh -c "source /etc/os-release && echo \$NAME")
distver=$(lsb_release -rs 2>/dev/null || sh -c "source /etc/os-release && echo \$VERSION_ID")

# Embed appropriate update info
# https://github.com/AppImage/AppImageSpec/blob/master/draft.md#github-releases
if [[ "$BUILD_REASON" == "Schedule" ]] ; then
  UPDATE="gh-releases-zsync|FlintyLemming|sideterm|nightly|SideTerm-*.AppImage.zsync"
  OUTPUT=SideTerm-nightly-$distro$distver.AppImage
else
  UPDATE="gh-releases-zsync|FlintyLemming|sideterm|latest|SideTerm-*.AppImage.zsync"
  OUTPUT=SideTerm-$TAG_NAME-$distro$distver.AppImage
fi

# Munge the path so that it finds our appstreamcli wrapper
PATH="$PWD/ci:$PATH" \
VERSION="$TAG_NAME" \
UPDATE_INFORMATION="$UPDATE" \
OUTPUT="$OUTPUT" \
  /tmp/linuxdeploy \
  --exclude-library='libwayland-client.so.0' \
  --appdir AppDir \
  --output appimage \
  --desktop-file assets/sideterm.desktop
