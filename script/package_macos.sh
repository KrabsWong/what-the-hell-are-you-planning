#!/bin/bash
# Produce a self-contained, ad-hoc signed Apple Silicon beta DMG.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || {
  echo 'This packaging script currently supports Apple Silicon macOS only.' >&2
  exit 1
}
cargo build --release --locked --offline
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' assets/brand/macos/Info.plist)
build_id=$(date -u +%Y%m%d%H%M%S)
output="$PWD/target/distribution"
staging=$(mktemp -d "$TMPDIR/planning-package.XXXXXX")
trap 'rm -rf "$staging"' EXIT
app="$staging/Planning.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$output"
cp target/release/what-the-hell-are-you-planning "$app/Contents/MacOS/"
cp assets/brand/macos/Info.plist "$app/Contents/Info.plist"
cp assets/brand/macos/Planning.icns "$app/Contents/Resources/"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build_id" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :LSMinimumSystemVersion string 11.0' "$app/Contents/Info.plist"
/usr/bin/plutil -lint "$app/Contents/Info.plist"
# Refuse a package that depends on developer-local dynamic libraries.
if otool -L "$app/Contents/MacOS/what-the-hell-are-you-planning" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/System/Library/|/usr/lib/)' ; then
  echo 'Non-system dynamic dependency found; bundle it before distributing.' >&2
  exit 1
fi
/usr/bin/codesign --force --sign - "$app"
/usr/bin/codesign --verify --strict --verbose=2 "$app"
ln -s /Applications "$staging/Applications"
cp LICENSE "$staging/LICENSE.txt"
cat > "$staging/安装说明.txt" <<'GUIDE'
Planning 内测版 · Apple Silicon (M 系列芯片)

安装
1. 将 Planning.app 拖入 Applications（应用程序）。
2. 推出磁盘映像，然后从 Applications 打开 Planning。
3. 首次启动会自动检测 OpenCode 和 ego(lite)；按提示安装缺少的依赖，选择自己的 Workspace。
4. 安装依赖后点击 Check again；三项通过后点击 Continue，并在 Settings 配置模型和服务商。

依赖需单独安装
OpenCode：https://opencode.ai
ego(lite)：https://lite.ego.app（安装后需打开并完成首次设置）
模型服务商可能产生费用，请使用自己的账号与配置。

签名与首次打开
这是未经过 Apple 公证、仅使用临时签名的内测版本。
从网络下载后 macOS 可能阻止打开。仅在确认文件来自可信分发者后，
根据 macOS 提示在“系统设置 → 隐私与安全性”允许打开。
受组织管理的 Mac 可能不允许运行未公证应用，请联系管理员。
Apple 官方说明：https://support.apple.com/en-us/102445

兼容性
仅包含 arm64，不支持 Intel Mac。构建最低系统版本为 macOS 11；
实际验证仅覆盖开发机系统，尚未在所有旧版本 macOS 上验证。

隐私
安装包不含开发者的工作区、任务、模型账号或凭据。
任务执行会使用用户自行配置的模型服务商。
GUIDE
name="Planning-${version}-beta-${build_id}-arm64"
hdiutil create -volname "Planning Beta" -srcfolder "$staging" -format UDZO -ov "$output/$name.dmg"
hdiutil verify "$output/$name.dmg"
(cd "$output" && shasum -a 256 "$name.dmg" > "$name.dmg.sha256")
printf '\nPackage: %s\nChecksum: %s\n' "$output/$name.dmg" "$output/$name.dmg.sha256"
