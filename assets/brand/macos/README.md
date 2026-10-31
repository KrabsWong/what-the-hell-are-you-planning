# macOS application icon

`Planning.icns` contains standard and Retina sizes of the static Assembly mark. `Planning.png` is the 1024px preview. The sidebar uses native animation independently.

Regenerate from the repository root on macOS:

```sh
swift script/generate_app_icon.swift /tmp/Planning.iconset
iconutil -c icns /tmp/Planning.iconset -o assets/brand/macos/Planning.icns
cp /tmp/Planning.iconset/icon_512x512@2x.png assets/brand/macos/Planning.png
```

Run `./script/build_and_run.sh` to build and launch `target/debug/Planning.app`. The bundle declares its icon and application identity in `Info.plist`. Direct `cargo run` launches the bare executable and does not supply the bundle's Dock icon. This is a local development bundle, not a signed/notarized distribution package.
