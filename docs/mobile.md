# The phone apps

Opencord for Android and iOS lives in `mobile/`. It is a React Native app (Expo SDK 58, React Native 0.88) on the same Rust client core the desktop app uses. The design is `docs/superpowers/specs/2026-10-26-mobile-client-design.md`; the plan building it is `docs/superpowers/plans/2026-10-26-mobile-client.md`.

## How it fits together

- **`crates/opencord-mobile`** is a UniFFI facade over `opencord_core::client::Client`:
  - JavaScript gets ids as decimal strings, times and sizes as numbers, and permissions as named flags (`ChannelAccess`).
  - Events are pulled in batches with `nextEvents`. `eventsAttach` starts a new generation, so a JavaScript reload takes the stream over from the old one.
- **`mobile/packages/core-native`** (`@opencord/core-native`) is the turbo-module uniffi-bindgen-react-native (`ubrn`, 0.31.0-6) builds from that crate. On Android it links the Rust library as a shared library, since a static one can't carry the system libraries cpal needs (`libandroid`, AAudio); on iOS as a static xcframework.
- **`mobile/src`** is the app:
  - `core/`: `CoreApi`, the real `NativeCore`, and `FakeCore`;
  - `state/`: the store, fed by the event loop;
  - `app/`: the routes;
  - the screens' folders.

## Using the app

### Installing it

- **Android 8.0 and up:** download `opencord-android.apk` from a release on GitHub and open it on the phone. Android asks to let your browser or files app install apps; allow it once. A newer APK installs over the old one and keeps your identity and servers. (A release carries the APK once it is signed with Opencord's key: docs/releasing.md, "Phone apps".)
- **iPhone:** not released yet. CI builds it for the simulator only; TestFlight comes later.

### Your identity

The first screen offers **Create an identity**, which asks only what people should call you, or **Import a backup**, a backup file made by the desktop app or by the phone. The identity is a key kept in the phone's keystore (Android) or keychain (iOS), not an account: back it up from **You → Back up your identity**, or losing the phone loses it.

### Servers

- **Add one** with **+** on the Servers tab. Paste an invite link (`opencord://…`) or type `host:port`. Opening an invite link on the phone opens this sheet too. A server with a self-signed certificate shows its fingerprint first; trust it only if it matches the one the server's owner gave you. A new server's first member pastes its **claim token** (under the field) to become its owner.
- **The list** shows each server's last message, unread and mention counts, and whether it is connected. A server you're in voice on shows the speaker beside it. Long-press a server to mark it as read, retry its connection, copy its address or leave it.

### Inside a server

- **The channel list** fills the screen: text channels, voice channels with who is in them, and what music plays.
- **Opening a text channel** slides the list away. Swipe right from the left half of the chat, or tap ☰, to bring it back as a sidebar; tap the chat, or swipe left, to close it. Back from the sidebar goes to the full list.
- **Chat:**
  - Type in the field at the bottom. It grows up to six lines, and the messages don't move while you type.
  - The clip button attaches photos and files.
  - Long-press a message to **Copy text**. On your own messages you can also **Edit** it, and **Delete** it, which people with the Manage messages permission can do to anyone's.
  - Scrolling down closes the keyboard. **Jump to latest** brings you back to the newest messages.
- **Voice messages:** hold the mic beside the field to record, and let go to send. Slide left to cancel, or slide up to lock the recording and keep both hands free; a locked recording can be stopped and heard before it is sent. Voice messages play in the chat at 1×, 1.5× or 2×.

### Voice

- **Join** from a voice channel's page. Mute, deafen, the soundboard and leave are at its foot.
- **While in voice**, a bar shows the channel (and what music plays) above the tab bar, and floats above the field in a chat. Tap it, or pull it up, to open the call; pull the call down to put it away.
- **Tap a person** to set how loud they are for you, or to mute them for you alone.
- **Voice keeps going** with the screen off or another app in front. Android shows a notification while it does.
- **The soundboard** plays a sound for everyone in the channel. Long-press one to **Preview** it for yourself, or to make it a favourite.

### Music

In a voice channel with music, **Music** opens the player at half height; pull it up for the whole queue. It shows what plays, with play and pause, skip, seek, shuffle and repeat, and your own music volume. Below that:
- **Up next:** swipe a song away, or long-press it to move it;
- **Search:** finds songs to add;
- **Recently played.**

The upload button sends audio files from the phone for the server to play, with progress, cancel and retry.

### The You tab

Your display name, **Back up your identity**, the theme (System, Light or Dark), and under Music your own music volume, **Mute music** and **Lower music while people talk**, then the app's version.

### Not there yet

| What | When |
|---|---|
| Camera, screen sharing, watching streams | The next phone plan: it needs the phones' video codecs. |
| Private messages and friends | A later plan. The core has them; the phone needs their screens. |
| Notifications when the app is closed | A later plan, with a push relay on the server. |
| Server administration: channels, roles, invites, settings | The desktop app does it. |
| Chat commands such as `/play` | Later. |
| A list of your own uploaded songs | Later: the server can't list a member's uploads yet. |
| Trusting a server again after its certificate changed | Later. Until then, leave the server (long-press it) and add it again. |

## Developing

### Once per machine

- **Node** 26 (`mobile/.nvmrc`) and **Yarn** 1.22.
- **Java** 17.
- **The Android SDK:**
  - with **NDK 27.1.12297006**, React Native 0.88's;
  - `mobile/scripts/env.sh` points at `$ANDROID_HOME`, or `~/sdk/android` when it isn't set.
- **Rust:**
  - the targets `x86_64-linux-android` and `aarch64-linux-android`;
  - on a Mac, also `aarch64-apple-ios` and `aarch64-apple-ios-sim`;
  - `cargo install cargo-ndk --locked`.
- **Emulators:** the phone emulator is `Pixel_7` (API 33, x86_64). `yarn emulator tablet` makes `Pixel_Tablet` from the same system image the first time.
- **Dependencies:** `cd mobile && yarn install`.

Every `yarn`, `npx`, `ubrn`, `adb`, `emulator` and Gradle command for the phone apps starts with `source mobile/scripts/env.sh &&`.

### Commands

From `mobile/`:

| What | Command |
|---|---|
| Build the core for the emulator (x86_64, debug) and regenerate the bindings | `yarn core:android` |
| The same, both ABIs in release (what CI ships) | `scripts/build-core.sh android --release` |
| Regenerate the bindings from the newest Android build | `yarn core:bindings` |
| Are the committed bindings current? | `scripts/check-bindings.sh` |
| Start an emulator | `yarn emulator` or `yarn emulator tablet` |
| Build and install the app | `yarn android` (first build about 15 minutes, then a couple) |
| The same on the fake world | `yarn android:fake` |
| Metro, for an installed build | `npx expo start --dev-client`, then `adb reverse tcp:8081 tcp:8081` |
| Type check, lint, tests | `yarn typecheck && yarn lint && yarn test` |
| Licences of production packages | `yarn licenses:check` |
| Screenshots of the running app | `scripts/screens.sh <task> <name>=<route> … [--tablet]` |
| The app icon, from Inter | `yarn icons` |

`.github/check.sh mobile` runs the facade's clippy and tests, the bindings check, and the app's checks. It needs an Android build of the core first, since the bindings are generated from one.

### Generated files

Never edit these by hand: regenerate them, and commit them with the change that caused it.

- **The bindings:**
  - `mobile/packages/core-native/src/generated/`
  - `mobile/packages/core-native/cpp/generated/`
- **The turbo-module glue ubrn writes beside them:**
  - `src/index.tsx`, `src/NativeCoreNative.ts`
  - `cpp/opencord-core-native.{cpp,h}`
  - `android/` (`build.gradle`, `CMakeLists.txt`, `cpp-adapter.cpp`, the Kotlin module and package, the manifest)
  - `ios/CoreNative.{h,mm}`
  - `CoreNative.podspec`

  It names the ABIs, so it always comes from the targets in `ubrn.config.yaml`. `scripts/build-core.sh` takes care of that, even after a one-ABI debug build.
- **Not committed:**
  - `mobile/android` and `mobile/ios`: `expo prebuild` makes them from `app.config.ts`;
  - the Rust libraries ubrn copies into the package (`jniLibs`, the xcframework);
  - the icons are committed, but `yarn icons` remakes them.

### A server for the emulator

Run one on this machine with `cargo run -p opencord-server`. The emulator reaches it at `10.0.2.2:7710`. Its certificate is self-signed, so adding it goes through the trust step.

### Voice on the emulator

- **The other side:** a voicebot in the same channel plays a tone and, when it stops, reports what it heard from each user: `cargo run -p opencord-voicebot -- --server "<invite>" --tone 440 --seconds 60`. The phone shows it speaking while its tone arrives.
- **The microphone:** the emulator (`yarn emulator` starts it with `-allow-host-audio`) records this machine's default input. To give it a voice, play speech into a null sink and move the emulator's capture there:
  - `pactl load-module module-null-sink sink_name=oc_null`;
  - `espeak-ng -w speech.wav "…"` and `paplay --device=oc_null speech.wav`;
  - `pactl move-source-output <qemu's index from pactl list short source-outputs> oc_null.monitor`;
  - afterwards, move it back and `pactl unload-module` the sink.

  The core's noise suppression removes a steady tone, so it has to be speech. The emulator's microphone arrives very quiet (about −61 dBFS at its peak), so loudness is checked on a real phone.
- **The core's logs** don't reach logcat. A panic on one of its threads is silent: look at what the system logs around it, such as AAudio opening and closing streams (`adb logcat | grep AAudio`).

### Without a server

`yarn android:fake` (or `EXPO_PUBLIC_FAKE_CORE=1` when starting Metro) runs the app on `FakeCore`'s fixed world, which uses its own identity:
- **Night Owls:** connected, with chat, voice and music.
- **Rust Club:** reconnecting.
- **Weekend Raid:** refused.

`EXPO_PUBLIC_FAKE_SCENARIO=empty` starts with no identity and no servers.

### Android's minimum is 8.0

The app needs Android 8.0 (API 26), AAudio's first version, which cpal needs for sound: linking against API 24 fails on `-laaudio`. `expo-build-properties` sets the app's `minSdkVersion` to 26.

### iOS's minimum is 16.4

That is Expo SDK 58's template. `scripts/build-core.sh ios` builds the core for it (`IPHONEOS_DEPLOYMENT_TARGET`). Without that, the C in the core's crates (Opus, ring, SQLite) is compiled for the SDK's own iOS version while Rust links for iOS 10, and the link fails on `___chkstk_darwin`.

React Native is built from source on iOS (`buildReactNativeFromSource` in `app.config.ts`), which makes the first build slower. `@react-native-menu/menu` 2.0.0 imports `"RCTBridge.h"` as a local header, and the precompiled React core doesn't provide it.

The simulator build is for Apple Silicon only: the core's xcframework has `aarch64-apple-ios` and `aarch64-apple-ios-sim` slices, so a simulator build passes `ARCHS=arm64` (CI does). An Intel Mac's simulator would need `x86_64-apple-ios` added to `ios.targets` in `ubrn.config.yaml`.
