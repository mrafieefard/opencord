# Releasing

Every push to `beta`, `canary` or `main` is a release. CI tests and builds the server and the app on Linux, Windows and macOS, publishes the server's Docker image, and puts it all on the repository's Releases page under a version it works out by itself. Nobody bumps a version by hand.

## Channels

| Branch | Channel | Tag | GitHub release | Docker tags |
| --- | --- | --- | --- | --- |
| `beta` | beta | `v0.0.2-beta.3` | Pre-release | `0.0.2-beta.3`, `beta` |
| `canary` | canary | `v0.0.2-canary.1` | Pre-release | `0.0.2-canary.1`, `canary` |
| `main` | stable | `v0.0.2` | Release, marked latest | `0.0.2`, `0.0`, `latest` (and `1` from 1.0.0 on) |

A change goes through the channels in order:

1. **Work happens on other branches** and reaches these through pull requests (CLAUDE.md). A pull request into `beta`, `canary` or `main` runs the whole workflow on the merge it would make: the tests and builds on all three systems, the integration tests, and the Docker image, built but not pushed. Its installers are uploaded as artifacts to try. It releases nothing. Run `.github/check.sh` before opening or updating one: it runs the Linux checks here.
2. **`beta`.** Merge finished work into `beta`, and every push makes the next beta, for testing.
3. **`canary`.** When a beta is good, merge `beta` into `canary`. It's the same code, numbered as a canary of the same version.
4. **`main`.** When the canary holds up, merge `canary` into `main`. That push releases the version.

Use real merges (`git merge --no-ff`, or GitHub's "Create a merge commit") between channels, never squash or rebase merges. The version is counted from the commits a release contains, and a squash makes new commits that look unreleased.

A fix that can't wait goes straight to `canary` or `main`. Merge it back down into `canary` and `beta` afterwards, so the channels don't drift apart.

## Versions

Versions are [Semantic Versioning 2.0.0](https://semver.org): `MAJOR.MINOR.PATCH`, with `-beta.N` or `-canary.N` on pre-releases. SemVer orders pre-release names alphabetically, so `0.0.2-beta.3 < 0.0.2-canary.1 < 0.0.2`, the same order as beta → canary → stable. Comparing two versions therefore always tells which build is further along, whatever the channel. The updater relies on this.

`.github/release/version.py next --branch <branch>` works out the version from the git tags and commit messages:

1. **The base** is the newest stable tag (`vX.Y.Z`). With none yet, the first version is **0.0.1**.
2. **The next version** comes from the commits since the base, read as [Conventional Commits](https://www.conventionalcommits.org):

   | Commits since the base | From 1.0.0 | Before 1.0.0 |
   | --- | --- | --- |
   | a breaking change: `feat!:`, `fix(voice)!:`, or a `BREAKING CHANGE:` footer | major: 1.4.2 → 2.0.0 | minor: 0.3.2 → 0.4.0 |
   | a `feat:` | minor: 1.4.2 → 1.5.0 | patch: 0.3.2 → 0.3.3 |
   | anything else: `fix:`, `perf:`, `docs:`, `chore:`, or no prefix at all | patch: 1.4.2 → 1.4.3 | patch: 0.3.2 → 0.3.3 |

   The largest one wins. Before 1.0.0 each step is one smaller, as SemVer allows while anything may still change.
3. **The pre-release number** comes next on `beta` and `canary`: one more than the last tag of that channel for the same version (`-beta.1`, `-beta.2`, …). It starts again at 1 when the version itself moves on, for example when a breaking change lands mid-beta and 0.0.2 becomes 0.1.0.
4. **`Release-As: x.y.z`** in a commit's footer chooses the version instead, if it's higher. This is how 1.0.0 will happen:

   ```
   chore: Opencord 1.0

   Release-As: 1.0.0
   ```

Merge commits don't count. Neither does a push with nothing new since the last stable release, for example merging `main` back into `beta`: CI still tests it but releases nothing.

A re-run of a release's workflow keeps that release's version and replaces its files, so re-running failed jobs is safe.

To see what a branch would release as, from a checkout with every tag fetched:

```sh
git fetch --tags
python3 .github/release/version.py next --branch beta
python3 .github/release/version.py notes --version 0.0.2-beta.3
```

### Where the version ends up

The committed files hold the last version written to them (Cargo's workspace version, `app/pubspec.yaml` and `appVersion` in `app/lib/core/app_info.dart`, which a test keeps equal). Local builds report that. In CI the Stamp step writes the release's version into all of them before building:

- the server reports it at `/info` and in its user agent;
- the core reports it in `core_version()`;
- the app shows it under User settings → About.

Windows' and macOS' file versions get the plain `X.Y.Z`, because they can't hold a pre-release name, with the run number as the build.

## What a run does

`.github/workflows/cicd.yml` (CI/CD):

| Job | What it does |
| --- | --- |
| Version | Works out the version, as above. |
| Server (Linux) | Checks `cargo fmt`, runs clippy and the tests of the server's crates, and builds `opencord-server-linux-x64.tar.gz`. |
| Server (Windows) | Runs the same checks, and builds `opencord-server-windows-x64.zip`. |
| Server (macOS) | Runs the same checks, and builds the universal `opencord-server-macos.tar.gz`. |
| Docker (amd64), Docker (arm64) | Builds `deploy/Dockerfile` on each architecture's own runner and pushes it to GHCR by digest. |
| Client (Linux) | Runs clippy and the tests of the app's crates, then `flutter analyze`, the app's tests and the integration tests, and builds `opencord-linux-x64.tar.gz`. |
| Client (Windows) | Runs clippy and the tests of the app's crates, and builds `opencord-windows-x64.zip`. |
| Client (macOS) | Runs clippy and the tests of the app's crates, and builds the universal `opencord-macos.zip`. |
| Phone (Android) | Type-checks, lints and tests the phone app, checks its bindings and licences, and builds `opencord-android.apk`, signed with Opencord's key (see "Phone apps" below). |
| Phone (iOS) | Builds the iPhone app for the simulator, unsigned. It is checked, not released. |
| Docker | Tags the image (`beta`, `canary`, `latest`, the version) once every job above has passed. |
| Release | Once the Docker job and both phone jobs have passed, leaves out the simulator app and a debug-signed APK, writes `release.json`, `SHA256SUMS` and the notes, and publishes the GitHub release. |

Nothing is tagged or released unless every test passes on every platform. Tests that need real hardware or millisecond timing skip on the shared runners (`OPENCORD_SHARED_RUNNER`): hardware encoders, VideoToolbox's bitrate, and speaking and frame-pacing timings.

The notes list the commits since the channel's previous release, grouped as breaking changes, features, fixes, performance and other. Beta notes start from the previous beta, canary notes from the previous canary, and a stable release's notes from the previous stable release.

## For the updater

The app's launcher (docs/updater.md) reads GitHub releases. Every release carries the same files under the same names, without the version in the name:

| Key | File |
| --- | --- |
| `client-linux-x64` | `opencord-linux-x64.tar.gz` |
| `client-windows-x64` | `opencord-windows-x64.zip` |
| `client-macos-universal` | `opencord-macos.zip` |
| `setup-windows-x64` | `opencord-setup-windows-x64.exe` |
| `setup-macos-universal` | `opencord-setup-macos.dmg` |
| `setup-linux-x64` | `opencord-setup-linux-x64` |
| `server-linux-x64` | `opencord-server-linux-x64.tar.gz` |
| `server-windows-x64` | `opencord-server-windows-x64.zip` |
| `server-macos-universal` | `opencord-server-macos.tar.gz` |
| `android-apk` | `opencord-android.apk`, when it was signed with Opencord's key |
|  | `SHA256SUMS` |
|  | `release.json` |
|  | `release.json.sig`, when the signing key is set |

`release.json` describes the release. Like Tauri's `latest.json` and electron-builder's `latest.yml`, it gives an updater everything in one small request:

```json
{
  "schema": 1,
  "version": "0.0.2-beta.3",
  "channel": "beta",
  "tag": "v0.0.2-beta.3",
  "commit": "9c1f…",
  "published_at": "2026-10-21T20:00:00Z",
  "notes_url": "https://github.com/mrafieefard/opencord/releases/tag/v0.0.2-beta.3",
  "platforms": {
    "client-linux-x64": {
      "file": "opencord-linux-x64.tar.gz",
      "url": "https://github.com/mrafieefard/opencord/releases/download/v0.0.2-beta.3/opencord-linux-x64.tar.gz",
      "size": 51234567,
      "sha256": "…"
    }
  },
  "docker": { "image": "ghcr.io/mrafieefard/opencord-server", "tags": ["0.0.2-beta.3", "beta"] }
}
```

How the updater should find an update:

1. **Stable users** fetch `https://github.com/mrafieefard/opencord/releases/latest/download/release.json`. "Latest" is always the newest stable release, it isn't rate-limited like the API, and it needs no token.
2. **Beta and canary users** list the releases with `GET https://api.github.com/repos/mrafieefard/opencord/releases?per_page=30` (60 requests an hour per address without a token, plenty for a check every few hours), then read the newest one's `release.json`. Each channel takes the highest version among the channels it follows:
   - beta follows beta, canary and stable;
   - canary follows canary and stable;
   - stable follows stable only.

   Because SemVer orders them, a beta user moves to the stable release once it's newer than their beta, and nobody is offered an older build.
3. **Offer the update** if that version is higher than `appVersion`. Download the platform's `url`, and check its `size` and `sha256` before replacing anything.
4. **Signing.** With the `UPDATE_SIGNING_KEY` secret set, CI signs `release.json` with Ed25519 into `release.json.sig`, and launchers built with the `UPDATE_PUBLIC_KEY` variable accept only signed releases, as Tauri's updater and Sparkle do. Without the secret, the trust comes from HTTPS to github.com. docs/updater.md says how to make the key.

## Phone apps

The phone apps take the release's version too: CI passes it to `mobile/app.config.ts` as `OPENCORD_VERSION`. Android also needs a version code, a number that only goes up, or a newer APK won't install over an older one. `mobile/plugins/versionCode.ts` makes it from the version, keeping SemVer's order:

```
major * 10 000 000 + minor * 100 000 + patch * 1 000 + rank * 100 + pre
```

`rank` is 0 for beta, 1 for canary and 9 for stable, and `pre` is the pre-release number. So `0.0.2-beta.3` is 2003, `0.0.2-canary.1` is 2101 and `0.0.2` is 2900. A minor, patch or pre-release number over 99 fails the build rather than making a code out of order.

**The signing key.** Android installs an update only when it is signed with the same key as the app on the phone. So every APK Opencord releases is signed with one key, made once and never lost: without it, everyone would have to uninstall the app, and lose their servers, to update. Make it on a trusted machine and keep it offline, as you do the update key (docs/updater.md):

```sh
keytool -genkeypair -v -keystore opencord-release.keystore -alias opencord \
  -keyalg RSA -keysize 4096 -validity 10000
```

Then give it to CI as four repository secrets (Settings → Secrets and variables → Actions):

```sh
base64 -w0 opencord-release.keystore | gh secret set ANDROID_KEYSTORE_BASE64
gh secret set ANDROID_KEYSTORE_PASSWORD   # the keystore's password
gh secret set ANDROID_KEY_ALIAS           # opencord
gh secret set ANDROID_KEY_PASSWORD        # the key's password
```

The Android job decodes the keystore into the runner's temporary folder, and Gradle reads the four values when it signs (`mobile/plugins/withReleaseSigning.ts`), so they never end up in the generated project. Without the secrets, the APK is signed with the debug key and named `opencord-android-unsigned-debugkey.apk`. It is still uploaded with the run, for testing, but the release leaves it out: no update signed with the real key could install over it.

The iPhone app isn't released yet. CI builds it for the simulator, unsigned. TestFlight and the App Store need an Apple Developer account and their own signing.

## Setup on GitHub

- **GHCR visibility.** The first run creates the `opencord-server` package, owned by the account and linked to the repository through the image's `org.opencontainers.image.source` label. If it shows as private under the account's Packages, make it public there (Package settings → Change visibility), so `docker pull` works without logging in.
- **Branch protection (suggested).** Protect `canary` and `main` from force pushes and deletion. Force-pushing a release branch rewrites commits that releases already point at.
