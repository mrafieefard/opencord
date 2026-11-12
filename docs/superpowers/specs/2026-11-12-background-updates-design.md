# Signed releases and background updates: design

2026-11-12. Desktop only: Windows, macOS and Linux. The phone apps update through their stores or a new APK and are not part of this.

The mockups the owner approved are beside this file:
- `2026-11-12-background-updates-square.html`: the update square (design C3).
- `2026-11-12-background-updates-animation.html`: the restart animation, on Apple's springs, with a slow-motion switch.

Open them in a browser. They are drawings of the intent, not code to copy.

## Why

Two things led to this.

**A security researcher reported that Opencord's updates are unsigned.** They are right. The launcher can check an Ed25519 signature on `release.json` (`crates/opencord-launcher/src/signature.rs`), and CI can sign it, but the repository has no key. So every launcher trusts HTTPS to github.com alone, and anyone who can replace a release's files can ship their own build to every user. `release.json` lists every file's SHA-256, so once it is signed, the downloads are covered too.

**Updates are forced.** Every start of the launcher checks GitHub, and downloads and unpacks any update before the app opens (docs/updater.md). Opening Opencord can wait on the network. The owner wants Opencord to open at once, update in the background, and offer the update quietly. The way it is offered should feel premium.

## Goals

1. Every release is signed, and launchers accept only releases signed with Opencord's key. A key that is set up wrong fails CI. It never breaks users' launchers.
2. Opencord opens at once on the installed version. Nothing waits for GitHub except the first install, a track switch, running the installer again, and the start after one that crashed (see "Recovering from a bad release").
3. Once the app has fully opened, updates download in the background without hurting a voice call. When one is ready, a square under the server list offers it.
4. Clicking the square restarts into the new version. A full-window animation covers the restart, the new version reverses it, and the user is back in the voice channel they were in.
5. If the user never clicks, the waiting update installs itself at the next start.

## Non-goals

- **Operating-system code signing.** That is Windows Authenticode and Apple notarization, which stop SmartScreen and Gatekeeper warning about the installer. It costs money and is separate work. The researcher's wording may have meant this instead, and the owner should ask them.
- Phones.
- Rotating or revoking the signing key.
- Rolling back to the previous version automatically when a new one fails to start.
- Applying an update without a restart.

## Anti-goals

Any of these would make this a failure, even if everything else works:
- An update download makes a voice call stutter or lag.
- Opencord installs a build from the wrong track, or a stale download long after a newer build came out.
- A CI or key mistake stops every launcher from updating, or crashes it.
- Opening Opencord waits on the network.
- The square nags: a badge, pulsing, a popup, a countdown, or anything that forces a click.
- The square or the animation looks glossy or shiny, or its colours differ between dark and light.

## Order

Stage 0 is signing. Stages 1 and later are the background updates. They are one plan, with signing first because:
- it is small and doesn't depend on anything else here;
- the background download checks signatures, and testing that needs signed releases to already exist.

## Stage 0: signing

### What exists

- **Launcher builds.** CI builds every launcher with `OPENCORD_UPDATE_PUBLIC_KEY` from the `UPDATE_PUBLIC_KEY` repository variable (`.github/workflows/cicd.yml`, the three `Launcher` steps).
- **Signing.** The `Sign` step signs `release.json` with the `UPDATE_SIGNING_KEY` secret when it is set.
- **Checking.** A launcher built with a key fetches `release.json.sig` and refuses a release whose signature doesn't hold (`github.rs`, `Github::manifest`).
- **What's missing.** Neither the secret nor the variable is set, and no release has a `release.json.sig`.

### Changes

Changes 1–6 are workflow and code. The key itself is set last, under "Turning it on".

1. **The `release` environment.** The release job runs in a GitHub environment named `release`. The environment holds `UPDATE_SIGNING_KEY`.
2. **CI refuses half a setup.** If only one of the secret and the variable is set, the release job fails before anything is released. The check has to run in the release job, the one in the `release` environment, because no other job, and no pull request run, can see an environment's secret.
3. **CI checks that the two keys match.** The run fails before anything is released when:
   - the public key derived from the secret differs from `UPDATE_PUBLIC_KEY` (compare the raw 32 bytes, since the variable may be raw or DER), or
   - `release.json.sig` doesn't verify against `UPDATE_PUBLIC_KEY` itself. Today the `Sign` step checks it only against the key it derived from the secret.
4. **A bad public key fails the build, not the launcher.** Today a key that doesn't parse panics in `signature::built_in_key` on every start of every user's launcher. Instead, parse and check it at build time (a `build.rs`, or a check in CI before `cargo build`), so a mistyped variable breaks the build.
5. **The app's build gets the key too.** Stage 1 checks signatures in `opencord-core`, which the `flutter build` steps compile on all three systems. Give those steps `OPENCORD_UPDATE_PUBLIC_KEY` as well.
6. **Docs.** Update the Signing section of docs/updater.md and docs/releasing.md, and add a decision to docs/decisions.md under the next free D number.

Only `beta`, `canary` and `main` may deploy to the environment, so no other branch's runs can read the secret. Today a repository secret would be readable by any workflow run in the repository. (My call.)

### Turning it on

**Order matters.**
- `UPDATE_PUBLIC_KEY` is a repository variable, so the moment it is set, every branch's next launcher build demands signatures.
- A branch still on the old workflow can't see a secret kept in the `release` environment. Its `Sign` step would warn and publish an unsigned release, which its own new launchers then refuse. On Windows and Linux they would already have replaced themselves, so that track would stop updating.

So:
1. The workflow changes reach `beta`, `canary` and `main`, through the usual pull requests from one track to the next.
2. Only then does the owner set the key. This happens once, on the owner's machine:
   1. Create the `release` environment (Settings → Environments), allowing only `beta`, `canary` and `main` to deploy.
   2. Run:
      ```sh
      mkdir -p ~/.config/opencord-release && cd ~/.config/opencord-release
      openssl genpkey -algorithm ed25519 -out update-key.pem && chmod 600 update-key.pem
      gh secret set UPDATE_SIGNING_KEY --env release --repo mrafieefard/opencord < update-key.pem
      openssl pkey -in update-key.pem -pubout -outform DER | openssl base64 -A \
        | gh variable set UPDATE_PUBLIC_KEY --repo mrafieefard/opencord
      ```
   3. Back `update-key.pem` up offline. There is no rotation, so if it is lost or leaked, every user has to reinstall by hand.

   The plan says this to the owner, and Claude doesn't run these commands.
3. The owner releases on every track soon afterwards, so each track has a signed release.

### What turning it on does to users

- **Windows and Linux:** installs get the checking launcher with their next update, because the launcher replaces itself from each new build.
- **macOS:** the launcher never updates itself, so it keeps not checking until the user installs a new disk image.
- **Switching tracks:** a checking launcher refuses unsigned releases. Switching to a track whose newest release is unsigned fails and is tried again at each start until that track has a signed release. The owner should release on every track soon after setting the key.

## Stage 1 and later: background updates

### Who does what

- **The launcher** starts the app at once, and switches to a waiting update first when there is one. A new `--prepare` mode unpacks a download the app hands it, and a new `--cover` mode holds the covered picture during an update restart. The launcher still handles the first install, track switches and uninstalling, with the splash as today.
- **The app** checks for updates, downloads them with the voice throttle, asks the launcher to prepare them, shows the square, plays the animation, restarts, and rejoins voice. The Rust side is in `opencord-core`, through flutter_rust_bridge, and the UI is Dart.
- **The shared code** moves out of the launcher's binary into a new library crate, `crates/opencord-update`. It holds:
  - the manifest (`release.rs`);
  - choosing a track's build (`track.rs`, and the selection from the releases list);
  - the signature check;
  - the size and hash check;
  - unpacking archives;
  - the install folder's JSON files, including the new ones below.

  Networking stays on each side: the launcher keeps blocking `reqwest`, and the core uses its async `reqwest` with the throttle. winit and the splash stay in the launcher, so the core never pulls them in.

**Why the launcher unpacks and the app doesn't.** The macOS app is sandboxed (`app/macos/Runner/Release.entitlements`). Files a sandboxed app writes are likely to be quarantined, and Gatekeeper would then refuse to open a quarantined, ad hoc signed `Opencord.app` when the launcher starts it. The launcher isn't sandboxed and sets no quarantine when it unpacks, which is why today's updates start. Unpacking in the launcher on every system keeps one code path. This needs checking on a real Mac (see "Checks owed").

### New files in the install folder

| File | Written by | Holds |
| --- | --- | --- |
| `downloads/<file>.part` | the app | a download in progress |
| `downloads/<file>.json` | the app | what that download is for: tag, version, track, URL, size, SHA-256 |
| `downloads/release.json`, `downloads/release.json.sig` | the app | the release being downloaded, which the launcher checks itself when it prepares it |
| `pending.json` | the launcher, after a prepare; the app only updates `confirmed_at` | `{version, track, prepared_at, confirmed_at}`; `app-<version>/` beside it is whole |
| `launching.json` | the launcher, just before it starts the app | `{version, at}`: the version it started |
| `started.json` | the app, once it has started (see below) | `{version, at}`: the version that got that far |

**"Started"** means `app_start` has finished and the first Flutter frame after `runApp` has been drawn (`WidgetsBinding.instance.endOfFrame`). That happens in every case: with the window shown, hidden in the tray (start minimized at login), or on the onboarding screen with no servers yet. It is the one point the crash marker and the update checks both count from. If they counted from the desktop shell showing, a user still in onboarding or an app hidden in the tray would never write the marker. That would mean a splash and a wait for GitHub on every start, and no update checks at all.
| `restart.json` | the app, just before an update restart | `{from, to, at, voice, window, cover}`: `voice` is `{server_key, channel_id}` or null, `window` the window's physical position and size, `cover` whether the launcher holds the cover |
| `cover.rgba` | the app, just before an update restart | the last covered frame, for the launcher's window |
| `cover.shown`, `cover.taken` | the launcher; the new app | the two hand-offs of the cover |

How they fit with what exists:
- **`current()` is unchanged.** `installed.json` still decides what starts. A prepared folder only starts after the launcher has switched `installed.json` to it.
- **`prune()` changes.** It keeps the current version, the newest other (as today) and the pending one. It stops deleting `downloads/` wholesale: it keeps the in-progress download for the newest release and removes everything else.
- **Every JSON file is written whole**, through a `.partial` file and a rename, as the existing ones are.

### Old launchers

The app must know whether its launcher understands `--prepare`. An old launcher passes arguments it doesn't know on to the app, so it would start a second Opencord. A new launcher sets `OPENCORD_LAUNCHER_FEATURES=prepare` when it starts the app; the variable is a space-separated list, so later features can be added. Without it, the app makes no background updates and behaves as today: the Restart button in Settings is the only way to update.

- **Windows and Linux:** by the time a new app runs, its launcher has already replaced itself from that build, so background updates work from the first new version.
- **macOS:** the launcher never updates itself, so macOS users get background updates once they install a new disk image. The release notes should say so.

The first session after a Windows or Linux install reaches its first new version still has no background updates: the launcher process that started it is the old one, even though it has just replaced its own file, so it doesn't set the variable. Background updates begin from the next start.

An old launcher also copes with what a new app leaves behind:
- it ignores `pending.json`;
- at its next update over the network it finds the prepared folder whole (`is_whole`) and switches to it without downloading.

### The launcher's start

On a plain start or a link, with something installed and the track unchanged:

1. **Switch to a waiting update** when `pending.json` names a version that:
   - has a whole folder,
   - is for the track in `update.json`,
   - is newer than the installed version, and
   - is fresh: prepared, or last confirmed by the app, less than 24 hours ago. An update restart (below) skips the age check, because the user has just asked for that build.

   The launcher then switches `installed.json` to it, updates itself from it (Windows and Linux, as `update_launcher` does today), deletes `pending.json` and prunes.
2. **Start the installed version at once**, with no network and no splash. The splash appears only if step 1 takes longer than a second. Write `launching.json` first.

**A waiting update that isn't fresh** is left alone, not switched to and not deleted. The app's next check decides:
- if it is still the newest build, the app writes `confirmed_at` into `pending.json` and shows the square at once with no download;
- if a newer build exists, that one replaces it.

This way a user who was away for two days doesn't download the same build twice, and a stale build never installs on its own. An app left running for days, for example in the tray, keeps confirming its waiting update at every check, so the update stays fresh and installs at the next start. The app's rule for showing the square and the launcher's rule for switching come from the same code in the shared crate, so they can't disagree.

**Recovering from a bad release.** Today a release that crashes on start is replaced by the next one automatically, because the launcher goes online on every start. Without a fallback, every user would be stuck on such a release. So the launcher still goes online first, with the splash and today's flow, in two cases:
- **It runs as the installer.** Running the installer again is what a user tries first, and it should repair the install.
- **The last start never got to "started".** `launching.json` names the installed version, but `started.json` doesn't. That catches a release that crashes before the app's first check, on every system, including a macOS user who dragged a new disk image over.

Both make that one start wait, which is the price of not stranding anyone. (My call.)

Unchanged:
- **Nothing installed:** the first install, with the splash.
- **A track switch waiting** (`update.json`'s track differs from `installed.json`'s): today's flow, with the splash. The user asked for it in Settings.
- `--uninstall`.
- `--after-pid` still waits for the old app to end.

**The update restart** is the app writing `restart.json` and running `launcher --after-pid <pid> --cover` (without `--cover` on Wayland). With `--cover`, the launcher first shows the cover and writes `cover.shown` (see "The launcher holds the cover"), and only then waits for the app to exit. Waiting first would deadlock, since the app waits for `cover.shown` before exiting. The launcher then takes step 1. When `restart.json` is less than 2 minutes old and its `to` is the waiting version, step 1 doesn't check that version's age. No splash should appear.

**`--prepare <downloads/<file>.json>`**:
1. Checks the signature of `downloads/release.json` when the launcher has a key.
2. Reads the manifest and checks that its tag, version, track and platform match.
3. Checks the archive's size and SHA-256.
4. Unpacks it to `app-<version>.partial`, checks the app executable is there, and renames the folder.
5. Writes `pending.json`.

It shows no window, starts nothing, doesn't prune, and doesn't touch `installed.json`. Only one prepare runs at a time, behind a lock file. It reports the result to the app with its exit code, and with a `downloads/<file>.error` file when it fails, because on macOS the app has to start it through `open` (see "Checks owed").

### The app: checking

- **When it checks:** only when the app has an `Installation` and the launcher lists `prepare`. A build run from an archive or from source never checks.
- **The first check:** 15 seconds after the app has started (as defined above), so it never competes with opening. (My call.) That includes an app hidden in the tray, and one still on the onboarding screen.
- **Later checks:** every 4 hours while the app stays open (my call). Each check makes one GitHub API call for beta and canary, well inside the 60-calls-an-hour limit for unauthenticated use.
- **Which build:** the launcher's rules, from the shared crate: each track takes only its own builds, and an older build than the installed one is never taken (D91).
- **Signature:** checked with the key built into the core (Stage 0, change 5). A build without a key trusts HTTPS, as the launcher does.
- **An update already waiting:** if `pending.json` already names the newest build, the square shows without downloading.
- **Errors** are silent. They add a line to the session log, and the next check tries again.

### The app: downloading, and the voice throttle

**Downloading.**
- **Chunks:** the archive comes in ranged chunks (HTTP `Range`, about 1 MiB each), appended to `downloads/<file>.part`.
- **Timeouts:** each chunk has its own timeout (about 30 s), instead of the launcher's 45 minutes for the whole download, since a throttled or paused download can take longer than that.
- **Resuming:** a download carries on after a pause or a restart of the app while `downloads/<file>.json` still matches the release. Otherwise it starts over.
- **Servers without `Range`:** GitHub's asset links redirect to a CDN that honours it. If a server ignores it and answers 200, the download starts over from the beginning.
- **The hash** is checked over the whole file once it is complete, by reading it back, because a resumed download can't be hashed in one pass.
- **When it's done:** the app writes `downloads/release.json` and its `.sig` beside the archive and runs the launcher with `--prepare`. If that succeeds, the square appears.

**A newer build while one is waiting.** The app downloads the newer one, and its prepare replaces `pending.json`. The square stays the whole time, because the older update is still a valid one, and its popover shows the newer version once that one is ready. The next start's prune removes the older folder. (My call.)

**The throttle.** It is active only while the user is in a voice call; outside one, the download runs at full speed. It reads the voice stats the core already gets once a second while a call is connected: ping, jitter, and loss in and out (`MediaEvent::ConnectionStats`; the interval is `STATS_EVERY` in `crates/opencord-media/src/transport/media.rs`). That interval stays as it is. The rules, with my thresholds:
- **Baseline:** the median ping of the 5 readings before the download starts. A call joined partway through a download has no ping until it connects, so the download pauses until 5 readings are in, takes their median, and then carries on at the starting rate.
- **Start:** 256 KiB/s in a call. Don't start at full speed.
- **Hurting** means a reading with ping more than 30 ms above the baseline, or more than 2 % of packets lost coming in.
- **Hurting → halve the rate**, to no less than 32 KiB/s. Still hurting at 32 KiB/s → pause until 3 readings in a row are calm.
- **Calm for 2 readings in a row → raise the rate by a quarter**, with no upper limit while it stays calm.
- **Leaving the call** brings back full speed at once.
- **The limit is enforced** by a token bucket on the chunk reads.

### The square

It is shown only while a prepared update is waiting. It is the approved design C3. The numbers below come from the mockup and the app's tokens.

- **Place:** its own float (`OcFloat`) under the server rail in the left column, 8 px below it. The rail gives up the room. The rail is always shown on desktop, so the square always has a place.
- **Size:** 62 × 62 with 16 px corners, like the other floats.
- **The tile inside:** 46 × 46 with 14 px corners. It is filled with the theme's `elevated` colour in dark (`#1F1F23`) or `hover` in light (`#F2F2F4`), plus a wash of the gradient at 16 % opacity. On hover the tile takes `selected` (`#27272C` dark, `#E7E7EA` light) and the wash goes to 24 %.
- **The icon:** Material Symbols Rounded `download` at 26 px, weight 500, filled with a 150° gradient from `#3D85F7` to `#8B5CF6`. These colours are the same in both themes. `download` and `check` must be added to the list in `tool/build_icons.dart`, which then regenerates `OcIcons`.
- **Arrival:** the rail shrinks and the square rises in on `.snappy`. The arrow drops in once on `.smooth`. Nothing pulses or loops afterwards.
- **Hover:**
  - the arrow drops 2 px on `.snappy`;
  - the rail's popover (`RailPopover`'s look) shows "Update ready", with "Click to restart on 0.0.2-beta.6" (the real version) under it.
- **Press:** the square scales to 96 % on `.snappy`.
- **Keyboard and screen readers:** the square can be focused and activated with Enter or Space. Its label reads "Update ready. Restart to update to <version>."
- **It is never dismissed and never asks for anything.** It waits.

### The restart animation

Every movement uses SwiftUI's own spring presets. Flutter 3.44's `SpringDescription.withDurationAndBounce` uses Apple's formula (stiffness = (2π / duration)², damping ratio = 1 − bounce), so the springs match Apple's exactly. The presets are `.smooth` (0.5 s, bounce 0), `.snappy` (0.5 s, bounce 0.15) and `.bouncy` (0.5 s, bounce 0.3). Some steps change only the duration, as `.smooth(duration:)` does in SwiftUI.

| Step | Spring |
| --- | --- |
| 1. The square lifts out of the rail and glides to the middle of the window, growing to 120 % | `.snappy` |
| 2. A wave spreads out from the square until it covers the window. A ring fades ahead of its edge. "Updating Opencord", with "This takes a few seconds" under it, fades in below the square once the wave is 85 % across | `.smooth(duration: 0.7)`; the caption `.smooth` |
| 3. The launcher takes over the cover and the app exits (see "The launcher holds the cover"). Nothing visibly changes | none |
| 4. The new version appears under the same cover | none |
| 5. The arrow turns into a check mark, and the caption changes to "Updated to <version>" | the icon `.bouncy`; the caption `.smooth` |
| 6. After about a second, the caption fades, the wave pulls back into the square, and the square glides home | the wave `.smooth(duration: 0.6)`; the glide `.snappy` |
| 7. The square folds away as the rail takes its room back | `.smooth` |

**The mockup differs in steps 3 and 4.** There the window fades and shrinks, and a grey screen stands for the restart. The real thing shows no change at all between the two versions: the owner chose to have the launcher hold the cover.

**The fill** is Gradient, the owner's choice: a 150° gradient from `#2D63D8` to `#6E47D3`. While the window is covered, the square, the icon and the captions are white. The mockup's Neutral fill is not used.

### The launcher holds the cover

Opencord's window appears only after its engine, keys and tray have started (`app_start.dart`), so between the old window closing and the new one appearing there would be a moment with no window at all, longer on slow machines. Fading or shrinking an OS window would also need native code on each system. Instead, the launcher shows a window with the covered picture on it, in the same place and size as the old window, and keeps it up until the new window is up underneath.

1. **The old app hands over the cover.** About 0.65 s after the caption has appeared:
   - It captures its own last covered frame. That is the whole window, title bar included, since the app draws its own.
   - It writes the frame to `cover.rgba` in the install folder: raw RGBA with a small header giving the width and height in physical pixels. The builder may use PNG instead.
   - It records the window's position and size, in physical pixels, in `restart.json`.
   - It starts `launcher --after-pid <pid> --cover`.
2. **The launcher shows the cover.** It opens a borderless window at exactly that place and size, draws the picture into it (winit and softbuffer, as the splash already does), and writes `cover.shown`.
3. **The old app exits.** It waits for `cover.shown`, for up to 3 seconds, then exits whether it appeared or not. Because the launcher's window shows the same picture, nothing visibly changes.
4. **The launcher switches and starts the new version**, as in "The update restart", keeping its window up.
5. **The new app opens already covered.**
   - It reads `restart.json` before it configures its window. If the file is less than 2 minutes old, its first frame is the covered picture: the gradient, the square in the middle with the download arrow, and "Updating Opencord".
   - Its window is shown only once that frame has been drawn. The Linux runner already shows the window on its first frame; the builder makes Windows and macOS do the same on this path.
   - As a safety net in case anything shows before the first frame, `startWindow`'s native background colour is the fill's middle, `#4D55D6`.
6. **The launcher steps aside.** The new app writes `cover.taken` once its window is showing. The launcher then closes its window and exits. If `cover.taken` doesn't appear within 30 seconds, the launcher closes its window anyway; a start that crashed is handled by "Recovering from a bad release".
7. **The new app finishes.** It plays steps 5–7 of the table, then deletes `restart.json`, `cover.rgba`, `cover.shown` and `cover.taken`.

**Wayland** doesn't let a program choose where its window goes, so the launcher can't place its window over the app's. When the app runs on Wayland, it records `"cover": false` and skips steps 1–3 and 6: the covered window closes, and the new one appears, already covered, a moment later. The window may also open somewhere else, since Wayland places it. (My call; there's no way around it on Wayland.)

**A failed update.** The app compares its own version (`Installation.version`) with `restart.json`'s `to`.
- **Equal:** the update worked, and steps 5–7 play as above.
- **Different:** the launcher didn't switch. The reverse still plays, but with the download arrow and no caption, and the square glides back and stays, because the update is still waiting. The app adds a line to the session log. (My call.)

**Reduce motion** (the OS setting or the app's, through `OcMotion`). Nothing moves. The cover fades in over 0.2 s, the launcher holds it in the same way, and the new version fades it out.

**Two smaller rules:**
- **Where the window reopens:** the existing "restore window position" setting decides. With it off, the reopened window may land somewhere else, and that is accepted.
- **The tray:** the restart must quit the app, never hide it to the tray.

### Rejoining voice

- **What's recorded:** when the user clicks the square, `restart.json` records the voice channel they are in (`VoiceSessionNotifier`'s server key and channel), and the app leaves the call.
- **When it rejoins:** the new app rejoins if `restart.json` is less than 2 minutes old and names a channel. It waits for that server to connect, for up to 30 seconds. (My calls.)
- **Even if the update failed:** it also rejoins on the failed path.
- **What carries over:**
  - mute and deafen stay as they were (they appear to be kept in local prefs already; the builder confirms);
  - camera and screen share stay off, as `join` already does.
- **If it can't rejoin:** the channel is gone, the user may not join it any more, or the server doesn't connect in time. In all three cases the user stays out and sees nothing. The owner said no note.
- **Only after an update restart.** An ordinary close and reopen doesn't rejoin.

### Unchanged

- Track switches, link opens, the first install and uninstalling.
- The Restart button in Settings, which now switches at once when an update is waiting.

## Testing

- **The shared crate:** the launcher's existing tests move with the code.
- **The launcher:**
  - whether a waiting update is switched to: fresh, confirmed recently, older than 24 hours, older but named by a fresh `restart.json`, wrong track, half-unpacked folder, older than installed;
  - going online first when run as the installer, and after a start that never wrote `started.json`, but not otherwise;
  - the cover's order: `--cover` shows the cover and writes `cover.shown` before it waits on the pid;
  - `--prepare` with a bad signature, a bad hash, the wrong platform, and a lock already held;
  - prune keeps the waiting version and the in-progress download;
  - `OPENCORD_LAUNCHER_FEATURES` is set;
  - `--cover` reads `cover.rgba`, writes `cover.shown`, and closes on `cover.taken` or after 30 seconds (the window itself can be faked behind a trait);
  - starting with no network makes no request.
- **The core:**
  - the throttle against scripted voice stats: hurting halves, hurting at the floor pauses, calm grows, leaving the call restores full speed, joining one takes a new baseline;
  - resuming a ranged download with a fake server, including one that ignores `Range`;
  - the hash over a resumed file;
  - choosing a build, as the launcher does.
- **Flutter:**
  - `started.json` is written, and the first check is scheduled, on a normal start, a start hidden in the tray, and a start on the onboarding screen;
  - the square appears only once an update is prepared, shows its popover on hover, and works from the keyboard;
  - goldens of the square in dark and light;
  - the animation's sequence of steps, driven by its controller;
  - `restart.json` on both the success and the failed path;
  - the rejoin logic against the fake repository.
- **By hand, owed:**
  - a real update between two signed releases on each system;
  - a voice call while a download runs, watching the ping;
  - the macOS checks below.

## Checks owed

These can't be done from the Linux machine.

- **macOS:**
  - Does a bundle the launcher unpacked, from an archive the sandboxed app downloaded, start without Gatekeeper complaining? If it doesn't, the launcher (which isn't sandboxed) removes `com.apple.quarantine` from the folders it unpacks.
  - Can the sandboxed app start the launcher with `--prepare` through `open -n -g` without the dock flashing, and learn its result?
  - Does an old launcher with a new app behave as "Old launchers" says?
- **Windows:**
  - Does `--prepare` cope with antivirus scanning the new folder while it is renamed?
- **The cover, on Windows, macOS and Linux on X11:**
  - Does the launcher's window land exactly over the app's, on a high-DPI screen and on a second monitor?
  - Is there no flash at either hand-off?
  - On macOS, does starting the launcher through `open` bring its window above the app's without the dock icon bouncing?

## Docs

- docs/updater.md: the Updating section, the new files, Signing, and the "Not done" list (updates while running is now done).
- docs/releasing.md.
- A decision in docs/decisions.md under the next free D number.

## The owner's decisions

- Updates are not forced. The app opens at once, checks after it has fully opened, downloads in the background, and offers the update with the square.
- A waiting update installs at the next start. A stale one doesn't.
- Downloads slow down when they raise the ping during a call.
- Clicking the square leaves the call straight away, with no warning, and the app rejoins the channel after the restart. If it can't rejoin, it shows nothing.
- The square is design C3:
  - a grey tile with a 14 px radius,
  - a faint wash of the gradient,
  - the arrow in a blue-to-violet gradient,
  - the same in dark and light,
  - not glossy.
- The restart animation: the square moves to the middle, a wave covers the window, the app closes, and the new version reverses it.
- The Gradient fill for the wave.
- The launcher holds the cover between the two versions, so the covered screen never disappears.
- Apple's spring curves, matched exactly, for every movement.
- Built in Flutter code, not with Rive.
- Signing and background updates in one plan, in the order Claude chose: signing first.

## Calls Claude made that the owner hasn't confirmed

- The cover is the old app's last frame saved as a picture, so the launcher shows exactly what the app showed. On Wayland the gap stays.
- Keeping a waiting update for 24 hours from its last confirmation before the launcher stops switching to it on its own; an update restart switches whatever its age.
- Going online first when the installer runs again and after a start that crashed, so a bad release can't strand users.
- The first check 15 seconds after the shell shows, then every 4 hours.
- The throttle's numbers: a baseline from 5 readings, 256 KiB/s to start, one reading at +30 ms or 2 % loss halves it, 32 KiB/s floor, ×1.25 after 2 calm readings, a pause for a baseline when a call is joined partway.
- Rejoining only within 2 minutes, waiting up to 30 seconds for the server; mute and deafen kept; camera and screen share off.
- On a failed update, the square comes back and the update stays waiting.
- The signing secret goes into a GitHub environment that only the release branches can use, and the key is set only once the workflow changes are on all three branches.
- The launcher unpacks on every system (`--prepare`), not only on macOS.

## Left to the builder

- How `opencord-update` is split into modules, and the names of its types.
- The chunk size and timeouts, within the limits above.
- How the app waits for `--prepare`'s result on each system.
- The wording of log lines.
