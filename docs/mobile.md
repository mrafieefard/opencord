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

### Without a server

`yarn android:fake` (or `EXPO_PUBLIC_FAKE_CORE=1` when starting Metro) runs the app on `FakeCore`'s fixed world, which uses its own identity:
- **Night Owls:** connected, with chat, voice and music.
- **Rust Club:** reconnecting.
- **Weekend Raid:** refused.

`EXPO_PUBLIC_FAKE_SCENARIO=empty` starts with no identity and no servers.

### Android's minimum is 8.0

The app needs Android 8.0 (API 26), AAudio's first version, which cpal needs for sound: linking against API 24 fails on `-laaudio`. `expo-build-properties` sets the app's `minSdkVersion` to 26.
