# Installer and updater

Opencord installs and updates itself the way Discord does. A small launcher (`crates/opencord-launcher`) is what the Start menu, the Dock, the desktop's app list and `opencord://` links start. It shows a splash, brings the app up to the newest build of the user's track from GitHub, and starts it. On Windows and Linux the same launcher is the installer. D71 records the decisions.

## What the user sees

They click Opencord. A small borderless window says **Opencord**, with a line under it that changes:

- "Checking for updates…"
- "Downloading update… 42%", with a bar
- "Installing update…"
- "Starting…"

When the app has started, the window closes. Escape closes it early, and the window can be dragged. The splash is drawn by hand into a pixel buffer (winit and softbuffer, Inter from `app/assets/fonts`), so it looks the same everywhere and needs no GPU. There is no logo yet.

Two cases skip parts of this:
- **Opened from a link.** If a version is installed, the launcher starts it at once and skips the check, so a link opens without waiting. The next plain launch updates.
- **GitHub unreachable.** The launcher gives up after 10 s and starts the installed version. With nothing installed yet, it says "Can't reach GitHub. Trying again…" and retries every 5 s.

## Where it installs

One folder per user, with no administrator needed:

| System | Folder | Launcher |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Opencord` | `Opencord.exe` in the folder |
| Linux | `$XDG_DATA_HOME/opencord` (`~/.local/share/opencord`) | `opencord` in the folder |
| macOS | `~/Library/Application Support/Opencord` | `/Applications/Opencord.app` |

The folder holds:
- **`app-<version>/`:** one folder per installed version, as its archive unpacks.
- **`update.json`:** the track.
- **`downloads/`**
- **`launcher.log`:** the last run's log. Read this first when an update goes wrong.

A version folder is unpacked under `app-<version>.partial` and renamed, so it appears whole or not at all. The newest one is the installed version. The one before it stays, and older ones are removed.

The app's own data (servers, messages, settings, keys) lives elsewhere, under `dev.opencord…`, so updates and uninstalling never touch it.

## Updating

Each time the launcher starts, it:

1. **Reads the track** from `update.json` (`{"track": "canary"}`).
2. **Finds the newest release the track takes**, as docs/releasing.md describes:
   - Stable reads `releases/latest/download/release.json`, which isn't rate-limited.
   - Canary and beta list the releases through the API and take the highest version among the channels they follow.
3. **If that is newer than the installed version:**
   - Downloads this system's archive.
   - Checks the size and SHA-256 that `release.json` gives.
   - Unpacks it beside the installed one.
4. **Updates the launcher itself (Windows and Linux)** from the copy a release carries in its app folder (`opencord-launcher`, `opencord-launcher.exe`). Windows can't overwrite a running executable, so the old one is renamed aside and removed on the next start.
5. **Starts the app**, passing on its own arguments (a link, `--autostart`). It also sets two variables:
   - `OPENCORD_LAUNCHER`: the launcher's path.
   - `OPENCORD_VERSION`: the installed version.

**A track never goes backwards.** Switching from beta to stable keeps the beta build until a stable release passes it, so nobody is moved to an older build. SemVer orders `0.0.2-beta.3 < 0.0.2-canary.1 < 0.0.2`, so "newer" means the same thing on every track.

**Signing.**
- **How it works.** CI signs `release.json` with an Ed25519 key when the repository has the `UPDATE_SIGNING_KEY` secret. The signature is published as `release.json.sig`.
- **What the launcher checks.** A launcher built with the matching `UPDATE_PUBLIC_KEY` variable accepts only signed releases. `release.json` lists every file's hash, so its signature covers the downloads.
- **Without the key.** The launcher trusts HTTPS to github.com alone, which is the case today.
- **Setting it up:**
  ```sh
  openssl genpkey -algorithm ed25519 -out update-key.pem
  openssl pkey -in update-key.pem -pubout -outform DER | openssl base64 -A   # the public key
  ```
  Put the PEM file's contents in the `UPDATE_SIGNING_KEY` secret and the base64 line in the `UPDATE_PUBLIC_KEY` variable (Settings → Secrets and variables → Actions). Keep the PEM file offline.
- **Order matters.** Releases must be signed from then on: a launcher with the key refuses unsigned ones.

## The track

The track is built into each release's launcher, from its branch: CI sets `OPENCORD_TRACK` to the channel. So the installer from a canary release installs canary.

- **Windows and Linux.** Running the installer sets the track in `update.json`, even over an existing install, since the user chose that installer.
- **macOS.** Installing is dragging the app over, so the launcher notes which track's disk image set the track (`installer_track`). A launcher from another track's disk image sets its own track on its first start. The macOS launcher doesn't update itself: a newer disk image replaces it.

**In the app**, Settings → About → Updates shows Stable, Canary and Beta. Saving writes `update.json`. "Restart" quits and has the launcher start the app again with whatever update is waiting (`--after-pid` waits for the old app to end first). A copy run from an archive or from source isn't installed: the section says it doesn't update itself.

## Installing

The release files (docs/releasing.md):

| System | Installer | Without installing |
| --- | --- | --- |
| Windows | `opencord-setup-windows-x64.exe` | `opencord-windows-x64.zip` |
| macOS | `opencord-setup-macos.dmg` | `opencord-macos.zip` |
| Linux | `opencord-setup-linux-x64` | `opencord-linux-x64.tar.gz` |

**Windows and Linux.** The launcher is the installer: run from anywhere but the install folder, it installs. It copies itself in, sets the track, and makes the app findable, then updates and starts the app like any launch. Everything is per user:
- **Windows:**
  - Start menu and desktop shortcuts. They carry the AppUserModelID `dev.opencord.opencord`, which the app sets on itself when the launcher started it, so pinning the app to the taskbar pins the launcher.
  - The `opencord://` and `opencords://` links.
  - An entry under Settings → Apps that runs `Opencord.exe --uninstall`. Uninstalling asks first; `--quiet` skips the question.
- **Linux:**
  - `~/.local/share/applications/dev.opencord.opencord.desktop`, the same file the app writes for itself, launching the launcher.
  - The links, through `xdg-mime`.
  - `opencord --uninstall` removes it.

**macOS.** The disk image holds the launcher as `Opencord.app`, to drag to Applications. It downloads the app on its first start.

**Where links and login go.** Started by the launcher, the app points everything at the launcher rather than at its version folder, which the next update removes:
- links (the Windows registry; the Linux desktop file, via `crates/opencord-media/src/hotkeys/portal.rs`)
- launch at login:
  - **Windows:** the Run key.
  - **Linux:** the autostart file.
  - **macOS:** a LaunchAgent, since a login item would name this version's bundle.

The macOS app is sandboxed. Two entitlement exceptions let it write `~/Library/Application Support/Opencord/` (for the track) and `~/Library/LaunchAgents/`.

## Not done

- **Code signing.** Windows SmartScreen warns about the unsigned installer, and macOS needs the quarantine removed from `Opencord.app` (docs/packaging.md). Ad hoc signing also means macOS asks for permissions again after each update, since each version is a new bundle.
- **Links on macOS** reach the installed app directly, through its own `Info.plist`, not through the launcher. They work, but don't update first. The launcher would need Apple Events to take them.
- **Updates while running.** The app checks only as it starts. Discord also checks every few hours and offers a restart; the "Restart" button is the manual version of that.
- **Rolling back.** The previous version stays on disk, but nothing switches back to it automatically if a new one fails to start.
- **A logo** for the splash, the shortcuts and the Linux desktop file.
