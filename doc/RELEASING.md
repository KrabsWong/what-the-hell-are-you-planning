# Releasing the macOS app

Release builds use Sparkle 2.9.3 for in-app updates. Every release publishes a
signed ZIP and an Ed25519-signed `appcast.xml` on GitHub Releases. The app reads
the stable feed:

```text
https://github.com/KrabsWong/what-the-hell-are-you-planning/releases/latest/download/appcast.xml
```

No Apple Developer certificate or notarization is required. The app and
framework are ad-hoc signed; ad-hoc signed updates keep the same identity.

## One-time repository setup

The release key pair is app-specific. The private seed belongs only in GitHub
Actions secrets and an encrypted backup; never commit it.

```sh
bash scripts/fetch-sparkle.sh
build/sparkle/sdk/bin/generate_keys --account what-the-hell-are-you-planning
build/sparkle/sdk/bin/generate_keys --account what-the-hell-are-you-planning -p
```

Store the printed public key as the Actions **variable**
`SPARKLE_PUBLIC_ED_KEY`, and put the same value in
`assets/brand/macos/Info.plist` under `SUPublicEDKey`. Export the seed and store
it as the Actions **secret** `SPARKLE_PRIVATE_ED_KEY`, then remove the plaintext
export:

```sh
build/sparkle/sdk/bin/generate_keys --account what-the-hell-are-you-planning -x /private/path/to/seed
```

The release script verifies that the seed derives the embedded public key before
it signs anything. Losing the private key requires a manual reinstall path.

## Publishing a release

1. Commit and merge the release commit into `main`.
2. Create and push a stable tag; the tag is the single source of truth for the
   version:

   ```sh
   git tag v0.1.1
   git push origin v0.1.1
   ```

3. The `Release macOS app` workflow first runs `script/set_version.sh` with the
   tag version, which writes `Cargo.toml`, `Cargo.lock` and
   `assets/brand/macos/Info.plist` in the runner. You do not need to edit these
   files or commit a version bump yourself; the tag must match the release
   commit on `main`.
4. The workflow then tests, builds the updater-enabled DMG, signs the
   ZIP/appcast, creates a draft release, uploads both assets, and marks it
   Latest only after both uploads succeed.

`script/set_version.sh MAJOR.MINOR.PATCH` can also be run locally when testing a
packaging change, but its edits are runner-local during a release and are never
pushed.

An existing stable release is never replaced. Rerunning a completed release
fails before publication rather than overwriting assets. Manual publication is
also available through **Actions → Release macOS app → Run workflow** with an
existing `vX.Y.Z` tag.

## Local packaging

The local DMG and the release ZIP share the same app bundle:

```sh
SPARKLE_PUBLIC_ED_KEY="$(build/sparkle/sdk/bin/generate_keys --account what-the-hell-are-you-planning -p)" \
  bash script/package_macos.sh
```

The DMG is written to `target/distribution/` and the updater ZIP plus appcast
are written to `dist/`.
