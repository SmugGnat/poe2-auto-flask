#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 3 ]]; then
    echo "Usage: $0 WINDOWS_ARTIFACT_DIR LINUX_LAUNCHER OUTPUT.AppImage" >&2
    exit 2
fi

windows_artifact=$(realpath -- "$1")
linux_launcher=$(realpath -- "$2")
output=$(realpath -m -- "$3")
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
package_dir=$(mktemp -d)
trap 'rm -rf -- "$package_dir"' EXIT

appdir="$package_dir/AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/lib/poe2-auto-flask" \
    "$appdir/usr/share/licenses/poe2-auto-flask" "$(dirname -- "$output")"
cp -- "$linux_launcher" "$appdir/usr/bin/poe2-auto-flask"
cp -- "$windows_artifact/poe2-auto-flask-windows-x64.exe" \
    "$appdir/usr/lib/poe2-auto-flask/poe2-auto-flask.exe"
cp -- "$windows_artifact/LICENSES.txt" "$appdir/usr/share/licenses/poe2-auto-flask/"
cp -- "$repo_root/LICENSE" "$appdir/usr/share/licenses/poe2-auto-flask/LICENSE"
cp -- "$repo_root/packaging/linux/AppRun" "$appdir/AppRun"
cp -- "$repo_root/packaging/linux/terminal-wrapper" "$appdir/usr/bin/poe2-auto-flask-terminal"
cp -- "$repo_root/packaging/linux/poe2-auto-flask.desktop" "$appdir/poe2-auto-flask.desktop"
cp -- "$repo_root/app-ui/ui/icon.png" "$appdir/poe2-auto-flask.png"
ln -s poe2-auto-flask.png "$appdir/.DirIcon"
chmod +x "$appdir/AppRun" "$appdir/usr/bin/poe2-auto-flask" \
    "$appdir/usr/bin/poe2-auto-flask-terminal"

# Optional local tool paths avoid downloads without changing the pinned tools.
if [[ -n ${APPIMAGETOOL:-} ]]; then
    cp -- "$APPIMAGETOOL" "$package_dir/appimagetool.AppImage"
else
    curl --fail --location --retry 3 --output "$package_dir/appimagetool.AppImage" \
        https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
fi
if [[ -n ${APPIMAGE_RUNTIME:-} ]]; then
    cp -- "$APPIMAGE_RUNTIME" "$package_dir/runtime-x86_64"
else
    curl --fail --location --retry 3 --output "$package_dir/runtime-x86_64" \
        https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
fi
printf '%s  %s\n' \
    ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 "$package_dir/appimagetool.AppImage" \
    2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d "$package_dir/runtime-x86_64" \
    | sha256sum --check -
chmod +x "$package_dir/appimagetool.AppImage"
ARCH=x86_64 "$package_dir/appimagetool.AppImage" --appimage-extract-and-run \
    --no-appstream --runtime-file "$package_dir/runtime-x86_64" "$appdir" "$output"
chmod +x "$output"
