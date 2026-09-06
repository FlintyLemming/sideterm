#!/usr/bin/env bash
set -x
set -e

TARGET_DIR=${1:-target}

TAG_NAME=${TAG_NAME:-$(bash ci/tag-name.sh)}

# Version fields inside the packages can't carry the leading "v" of a tag like
# v0.1.0 -- dpkg requires a version that starts with a digit. File names keep
# using TAG_NAME; only the metadata uses this.
PKG_VERSION=${TAG_NAME#nightly-}
PKG_VERSION=${PKG_VERSION#nightly}
PKG_VERSION=${PKG_VERSION#v}

HERE=$(pwd)

if test -z "${SUDO+x}" && hash sudo 2>/dev/null; then
  SUDO="sudo"
fi

if test -e /etc/os-release; then
  . /etc/os-release
fi

echo "OSTYPE is $OSTYPE"

case $OSTYPE in
  darwin*)
    zipdir=SideTerm-macos-$TAG_NAME
    if [[ "$BUILD_REASON" == "Schedule" ]] ; then
      zipname=SideTerm-macos-nightly.zip
    else
      zipname=$zipdir.zip
    fi
    rm -rf $zipdir $zipname
    mkdir $zipdir
    cp -r assets/macos/SideTerm.app $zipdir/
    # Omit MetalANGLE for now; it's a bit laggy compared to CGL,
    # and on M1/Big Sur, CGL is implemented in terms of Metal anyway
    rm $zipdir/SideTerm.app/*.dylib
    mkdir -p $zipdir/SideTerm.app/Contents/MacOS
    mkdir -p $zipdir/SideTerm.app/Contents/Resources
    cp -r assets/shell-integration/* $zipdir/SideTerm.app/Contents/Resources
    cp -r assets/shell-completion $zipdir/SideTerm.app/Contents/Resources
    tic -xe wezterm -o $zipdir/SideTerm.app/Contents/Resources/terminfo termwiz/data/wezterm.terminfo

    for bin in wezterm:sideterm wezterm-mux-server:sideterm-mux-server wezterm-gui:sideterm-gui strip-ansi-escapes:strip-ansi-escapes ; do
      src=${bin%%:*}
      dest=${bin##*:}
      # If the user ran a simple `cargo build --release`, then we want to allow
      # a single-arch package to be built
      if [[ -f $TARGET_DIR/release/$src ]] ; then
        cp $TARGET_DIR/release/$src $zipdir/SideTerm.app/Contents/MacOS/$dest
      else
        # The CI runs `cargo build --target XXX --release` which means that
        # the binaries will be deployed in `$TARGET_DIR/XXX/release` instead of
        # the plain path above.
        # In that situation, we have two architectures to assemble into a
        # Universal ("fat") binary, so we use the `lipo` tool for that.
        lipo $TARGET_DIR/*/release/$src -output $zipdir/SideTerm.app/Contents/MacOS/$dest -create
      fi
    done

    set +x
    if [ -n "$MACOS_TEAM_ID" ] ; then
      MACOS_PW=$(echo $MACOS_CERT_PW | base64 --decode)
      echo "pw sha"
      echo $MACOS_PW | shasum

      # Remove pesky additional quotes from default-keychain output
      def_keychain=$(eval echo $(security default-keychain -d user))
      echo "Default keychain is $def_keychain"
      echo "Speculative delete of build.keychain"
      security delete-keychain build.keychain || true
      echo "Create build.keychain"
      security create-keychain -p "$MACOS_PW" build.keychain
      echo "Make build.keychain the default"
      security default-keychain -d user -s build.keychain
      echo "Unlock build.keychain"
      security unlock-keychain -p "$MACOS_PW" build.keychain
      echo "Import .p12 data"
      echo $MACOS_CERT | base64 --decode > /tmp/certificate.p12
      echo "decoded sha"
      shasum /tmp/certificate.p12
      security import /tmp/certificate.p12 -k build.keychain -P "$MACOS_PW" -T /usr/bin/codesign
      rm /tmp/certificate.p12
      echo "Grant apple tools access to build.keychain"
      security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$MACOS_PW" build.keychain
      echo "Codesign"
      /usr/bin/codesign --keychain build.keychain --force --options runtime \
        --entitlements ci/macos-entitlement.plist --deep --sign "$MACOS_TEAM_ID" $zipdir/SideTerm.app/
      echo "Restore default keychain"
      security default-keychain -d user -s $def_keychain
      echo "Remove build.keychain"
      security delete-keychain build.keychain || true
    fi

    set -x
    zip -r $zipname $zipdir
    set +x

    if [ -n "$MACOS_TEAM_ID" ] ; then
      echo "Notarize"
      xcrun notarytool submit $zipname --wait --team-id "$MACOS_TEAM_ID" --apple-id "$MACOS_APPLEID" --password "$MACOS_APP_PW"
    fi
    set -x

    ;;
  msys|cygwin)
    zipdir=SideTerm-windows-$TAG_NAME
    if [[ "$BUILD_REASON" == "Schedule" ]] ; then
      zipname=SideTerm-windows-nightly.zip
      instname=SideTerm-nightly-setup
    else
      zipname=$zipdir.zip
      instname=SideTerm-${TAG_NAME}-setup
    fi
    rm -rf $zipdir $zipname
    mkdir $zipdir
    cp $TARGET_DIR/release/wezterm.exe $zipdir/sideterm.exe
    cp $TARGET_DIR/release/wezterm-mux-server.exe $zipdir/sideterm-mux-server.exe
    cp $TARGET_DIR/release/wezterm-gui.exe $zipdir/sideterm-gui.exe
    cp $TARGET_DIR/release/wezterm.pdb $zipdir/sideterm.pdb
    cp $TARGET_DIR/release/strip-ansi-escapes.exe \
      assets/windows/conhost/conpty.dll \
      assets/windows/conhost/OpenConsole.exe \
      assets/windows/angle/libEGL.dll \
      assets/windows/angle/libGLESv2.dll \
      $zipdir
    mkdir $zipdir/mesa
    cp $TARGET_DIR/release/mesa/opengl32.dll \
        $zipdir/mesa
    7z a -tzip $zipname $zipdir
    iscc.exe -DMyAppVersion=${PKG_VERSION} -F${instname} ci/sideterm-installer.iss
    ;;
  linux-gnu|linux)
    distro=$(lsb_release -is 2>/dev/null || sh -c "source /etc/os-release && echo \$NAME")
    distver=$(lsb_release -rs 2>/dev/null || sh -c "source /etc/os-release && echo \$VERSION_ID")
    case "$distro" in
      *Fedora*|*CentOS*|*SUSE*)
        WEZTERM_RPM_VERSION=$(echo $PKG_VERSION | tr - _)
        distroid=$(sh -c "source /etc/os-release && echo \$ID" | tr - _)
        distver=$(sh -c "source /etc/os-release && echo \$VERSION_ID" | tr - _)

        SPEC_RELEASE="1.${distroid}${distver}"
        if test -n "${COPR_SRPM}" ; then
          SPEC_RELEASE=0
        fi

        # Set up variables for spec generation
        if test -n "${COPR_SRPM}" ; then
          TAR_NAME=$(git -c "core.abbrev=8" show -s "--format=%cd_%h" "--date=format:%Y%m%d_%H%M%S")
          HERE="."
          BUILD_SECTION=$(cat <<'BUILDEOFEOF'
%prep
%autosetup
%build

echo Here I am

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

cargo build --release \
      -p wezterm-gui -p wezterm -p wezterm-mux-server \
      -p strip-ansi-escapes
BUILDEOFEOF
)
          BUILD_REQUIRES=$(cat <<BREQEOF
BuildRequires: gcc, gcc-c++, make, curl, fontconfig-devel, openssl-devel, libxcb-devel, libxkbcommon-devel, libxkbcommon-x11-devel, wayland-devel, xcb-util-devel, xcb-util-keysyms-devel, xcb-util-image-devel, xcb-util-wm-devel, git
%if 0%{?suse_version}
BuildRequires: Mesa-libEGL-devel
%else
BuildRequires: mesa-libEGL-devel
%endif
%if 0%{?fedora} >= 41 && 0%{?fedora} < 45
BuildRequires: openssl-devel-engine
%endif
Source0: sideterm-${TAR_NAME}.tar.gz
BREQEOF
)
        else
          HERE="${HERE}"
          BUILD_SECTION=$(cat <<'BUILDEOFEOF'
%build
echo build
BUILDEOFEOF
)
          BUILD_REQUIRES=""
        fi

        # Generate single spec with subpackages
        cat > sideterm.spec <<EOF
Name: sideterm
Version: ${WEZTERM_RPM_VERSION}
Release: ${SPEC_RELEASE}
License: MIT
URL: https://github.com/FlintyLemming/sideterm
Summary: SideTerm terminal emulator.
${BUILD_REQUIRES}
Requires: sideterm-common, sideterm-gui, sideterm-mux-server

%global debug_package %{nil}

%description
sideterm is a terminal emulator with a workspace sidebar, built on
wezterm, with support for modern features such as fonts with
ligatures, hyperlinks, tabs and multiple windows.

# Subpackage: sideterm-common
%package -n sideterm-common
Summary: SideTerm - Common CLI components
Requires: openssl
%description -n sideterm-common
sideterm-common provides the base CLI launcher and utilities shared by
all sideterm components.

# Subpackage: sideterm-gui
%package -n sideterm-gui
Summary: SideTerm - GUI and multiplexer
Requires: sideterm-common
%if 0%{?suse_version}
Requires: dbus-1, fontconfig, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-egl1, libwayland-cursor0, Mesa-libEGL1, libxcb-keysyms1, libxcb-ewmh2, libxcb-icccm4
%else
Requires: dbus, fontconfig, libxcb, libxkbcommon, libxkbcommon-x11, libwayland-client, libwayland-egl, libwayland-cursor, mesa-libEGL, xcb-util-keysyms, xcb-util-wm
%endif
%description -n sideterm-gui
sideterm-gui is a GPU-accelerated cross-platform terminal emulator with
a workspace sidebar and support for modern features such as fonts with
ligatures, hyperlinks, tabs and multiple windows.

# Subpackage: sideterm-mux-server
%package -n sideterm-mux-server
Summary: SideTerm - Multiplexer server (headless)
Requires: openssl
%description -n sideterm-mux-server
sideterm-mux-server is a headless terminal multiplexer that can be used
as a session manager for terminal sessions, without requiring X11,
Wayland, or other GUI libraries.

${BUILD_SECTION}

%install
set -x
cd ${HERE}
mkdir -p %{buildroot}/usr/bin %{buildroot}/etc/profile.d %{buildroot}/usr/share/icons/hicolor/128x128/apps %{buildroot}/usr/share/applications %{buildroot}/usr/share/metainfo %{buildroot}/usr/share/nautilus-python/extensions
install -Dm755 assets/open-sideterm-here -t %{buildroot}/usr/bin
install -Dsm755 $TARGET_DIR/release/wezterm %{buildroot}/usr/bin/sideterm
install -Dsm755 $TARGET_DIR/release/wezterm-gui %{buildroot}/usr/bin/sideterm-gui
install -Dsm755 $TARGET_DIR/release/wezterm-mux-server %{buildroot}/usr/bin/sideterm-mux-server
install -Dsm755 $TARGET_DIR/release/strip-ansi-escapes -t %{buildroot}/usr/bin
install -Dm644 assets/shell-integration/* -t %{buildroot}/etc/profile.d
install -Dm644 assets/shell-completion/zsh %{buildroot}/usr/share/zsh/site-functions/_sideterm
install -Dm644 assets/shell-completion/bash %{buildroot}/etc/bash_completion.d/sideterm
install -Dm644 assets/icon/terminal.png %{buildroot}/usr/share/icons/hicolor/128x128/apps/org.sideterm.sideterm.png
install -Dm644 assets/sideterm.desktop %{buildroot}/usr/share/applications/org.sideterm.sideterm.desktop
install -Dm644 assets/sideterm.appdata.xml %{buildroot}/usr/share/metainfo/org.sideterm.sideterm.appdata.xml
install -Dm644 assets/sideterm-nautilus.py %{buildroot}/usr/share/nautilus-python/extensions/sideterm-nautilus.py

%files
# Main package (metapackage) has no files

%files -n sideterm-common
/usr/bin/sideterm
/usr/bin/strip-ansi-escapes
/usr/share/zsh/site-functions/_sideterm
/etc/bash_completion.d/sideterm
/etc/profile.d/*

%files -n sideterm-gui
/usr/bin/open-sideterm-here
/usr/bin/sideterm-gui
/usr/share/icons/hicolor/128x128/apps/org.sideterm.sideterm.png
/usr/share/applications/org.sideterm.sideterm.desktop
/usr/share/metainfo/org.sideterm.sideterm.appdata.xml
/usr/share/nautilus-python/extensions/sideterm-nautilus.py*

%files -n sideterm-mux-server
/usr/bin/sideterm-mux-server

%changelog
* Mon Oct 2 2023 SideTerm
- See git for full changelog
EOF

        if test -n "${COPR_SRPM}" ; then
          /usr/bin/rpmbuild -bs --rmspec sideterm.spec --verbose
          mv $(rpm --eval '%{_srcrpmdir}')/sideterm-${TAR_NAME}*.src.rpm "${COPR_SRPM}"/
        else
          /usr/bin/rpmbuild -bb --rmspec sideterm.spec --verbose
        fi

        ;;
      Ubuntu*|Debian*|Pop)
        rm -rf pkg
        mkdir -p pkg/debian/usr/bin pkg/debian/DEBIAN pkg/debian/usr/share/{applications,sideterm}

        if [[ "$BUILD_REASON" == "Schedule" ]] ; then
          pkgname=sideterm-nightly
          conflicts=sideterm
        else
          pkgname=sideterm
          conflicts=sideterm-nightly
        fi

        cat > pkg/debian/control <<EOF
Package: $pkgname
Version: ${PKG_VERSION}
Conflicts: $conflicts
Architecture: $(dpkg-architecture -q DEB_BUILD_ARCH_CPU)
Maintainer: FlintyLemming <admin@flinty.moe>
Section: utils
Priority: optional
Homepage: https://github.com/FlintyLemming/sideterm
Description: SideTerm terminal emulator.
 sideterm is a terminal emulator with a workspace sidebar, built on
 wezterm, with support for modern features such as fonts with
 ligatures, hyperlinks, tabs and multiple windows.
Provides: x-terminal-emulator
Source: https://github.com/FlintyLemming/sideterm
EOF

        cat > pkg/debian/postinst <<EOF
#!/bin/sh
set -e
if [ "\$1" = "configure" ] ; then
        update-alternatives --install /usr/bin/x-terminal-emulator x-terminal-emulator /usr/bin/open-sideterm-here 20
fi
EOF

        cat > pkg/debian/prerm <<EOF
#!/bin/sh
set -e
if [ "\$1" = "remove" ]; then
	update-alternatives --remove x-terminal-emulator /usr/bin/open-sideterm-here
fi
EOF

        install -Dsm755 $TARGET_DIR/release/wezterm-mux-server pkg/debian/usr/bin/sideterm-mux-server
        install -Dsm755 $TARGET_DIR/release/wezterm-gui pkg/debian/usr/bin/sideterm-gui
        install -Dsm755 $TARGET_DIR/release/wezterm pkg/debian/usr/bin/sideterm
        install -Dm755 -t pkg/debian/usr/bin assets/open-sideterm-here
        install -Dsm755 -t pkg/debian/usr/bin $TARGET_DIR/release/strip-ansi-escapes

        deps=$(cd pkg && dpkg-shlibdeps -O -e debian/usr/bin/*)
        mv pkg/debian/postinst pkg/debian/DEBIAN/postinst
        chmod 0755 pkg/debian/DEBIAN/postinst
        mv pkg/debian/prerm pkg/debian/DEBIAN/prerm
        chmod 0755 pkg/debian/DEBIAN/prerm
        mv pkg/debian/control pkg/debian/DEBIAN/control
        sed -i '/^Source:/d' pkg/debian/DEBIAN/control  # The `Source:` field needs to be valid in a binary package
        echo $deps | sed -e 's/shlibs:Depends=/Depends: /' >> pkg/debian/DEBIAN/control
        cat pkg/debian/DEBIAN/control

        install -Dm644 assets/icon/terminal.png pkg/debian/usr/share/icons/hicolor/128x128/apps/org.sideterm.sideterm.png
        install -Dm644 assets/sideterm.desktop pkg/debian/usr/share/applications/org.sideterm.sideterm.desktop
        install -Dm644 assets/sideterm.appdata.xml pkg/debian/usr/share/metainfo/org.sideterm.sideterm.appdata.xml
        install -Dm644 assets/sideterm-nautilus.py pkg/debian/usr/share/nautilus-python/extensions/sideterm-nautilus.py
        install -Dm644 assets/shell-completion/bash pkg/debian/usr/share/bash-completion/completions/sideterm
        install -Dm644 assets/shell-completion/zsh pkg/debian/usr/share/zsh/functions/Completion/Unix/_sideterm
        install -Dm644 assets/shell-integration/* -t pkg/debian/etc/profile.d

        if [[ "$BUILD_REASON" == "Schedule" ]] ; then
          debname=sideterm-nightly.$distro$distver
        else
          debname=sideterm-$TAG_NAME.$distro$distver
        fi
        arch=$(dpkg-architecture -q DEB_BUILD_ARCH_CPU)
        case $arch in
          amd64)
            ;;
          *)
            debname="${debname}.${arch}"
            ;;
        esac

        fakeroot dpkg-deb --build pkg/debian $debname.deb

        if [[ "$BUILD_REASON" != '' ]] ; then
          $SUDO apt-get install ./$debname.deb
        fi

        mv pkg/debian pkg/sideterm
        tar cJf $debname.tar.xz -C pkg sideterm
        rm -rf pkg
      ;;
    esac
    ;;
  linux-musl)
    case $ID in
      alpine)
        export SUDO=''
        abuild-keygen -a -n -b 8192
        pkgver="$PKG_VERSION"
        cat > APKBUILD <<EOF
# Maintainer: FlintyLemming <admin@flinty.moe>
pkgname=sideterm
pkgver=$(echo "$pkgver" | cut -d'-' -f1-2 | tr - .)
_pkgver=$pkgver
pkgrel=0
pkgdesc="A GPU-accelerated terminal emulator with a workspace sidebar"
license="MIT"
arch="all"
options="!check"
url="https://github.com/FlintyLemming/sideterm"
makedepends="cmd:tic"
source="
  $TARGET_DIR/release/wezterm
  $TARGET_DIR/release/wezterm-gui
  $TARGET_DIR/release/wezterm-mux-server
  assets/open-sideterm-here
  assets/sideterm.desktop
  assets/sideterm.appdata.xml
  assets/icon/terminal.png
  assets/icon/wezterm-icon.svg
  termwiz/data/wezterm.terminfo
"
builddir="\$srcdir"

build() {
  tic -x -o "\$builddir"/wezterm.terminfo "\$srcdir"/wezterm.terminfo
}

package() {
  install -Dm755 -t "\$pkgdir"/usr/bin "\$srcdir"/open-sideterm-here
  install -Dm755 "\$srcdir"/wezterm "\$pkgdir"/usr/bin/sideterm
  install -Dm755 "\$srcdir"/wezterm-gui "\$pkgdir"/usr/bin/sideterm-gui
  install -Dm755 "\$srcdir"/wezterm-mux-server "\$pkgdir"/usr/bin/sideterm-mux-server

  install -Dm644 -t "\$pkgdir"/usr/share/applications "\$srcdir"/sideterm.desktop
  install -Dm644 -t "\$pkgdir"/usr/share/metainfo "\$srcdir"/sideterm.appdata.xml
  install -Dm644 "\$srcdir"/terminal.png "\$pkgdir"/usr/share/pixmaps/sideterm.png
  install -Dm644 "\$srcdir"/wezterm-icon.svg "\$pkgdir"/usr/share/pixmaps/sideterm.svg
  install -Dm644 "\$srcdir"/terminal.png "\$pkgdir"/usr/share/icons/hicolor/128x128/apps/sideterm.png
  install -Dm644 "\$srcdir"/wezterm-icon.svg "\$pkgdir"/usr/share/icons/hicolor/scalable/apps/sideterm.svg
  install -Dm644 "\$builddir"/wezterm.terminfo "\$pkgdir"/usr/share/terminfo/w/wezterm
}
EOF
        abuild -F checksum
        abuild -Fr
      ;;
    esac
    ;;
  *)
    ;;
esac
