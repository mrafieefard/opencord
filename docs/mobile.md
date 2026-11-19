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

## Plugins on the phone

A plugin channel's screen and an activity are drawn by the same renderer the desktop uses (`app/packages/opencord_blocks`), running in Flutter engines inside the React Native app. Each area is one engine, from `createArea` to `destroyArea`, whatever view shows it. At most two are alive: a plugin channel and an activity.

```
core (crates/opencord-mobile: view_open, view_input, next_view_outputs)
  │  batches pulled by generation, inputs pushed
  ▼
JavaScript (mobile/src/features/plugins: viewPump.ts, viewSession.ts, PluginArea.tsx,
            hostCalls.ts + phoneHost.ts, activityHolder.ts)
  │  the JSON strings untouched
  ▼
native module (mobile/modules/plugin-area: PluginAreaModule.kt/.swift, Areas, AreaPipe,
               PluginAreaView): an engine group, a spare engine, one MethodChannel per area
  │  dev.opencord/plugin_area: setup, batch, hostReply, back ↔ ready, input, status, hostCall
  ▼
Dart (mobile/flutter_module: area.dart, channel.dart, phone_host.dart, overlays.dart)
  └─ BlocksView from opencord_blocks, at a 44-point touch size
```

- **What JavaScript does:** opens a view on the renderer's first context input; pumps batches to the area that owns them, holding early ones until the view's id is known; answers host calls (images, fonts, users, pickers, uploads, the link question, copy, opening channels, sounds) with the phone's own controls; tells the plugin `pause` and `resume`.
- **Back:** Android's back asks the area first, while its screen is in front (a sheet pushed over it takes its own back). It closes the plugin's top dialog, modal or sheet, then asks the plugin's `app.on_back`, and leaves the screen only when neither went back. iOS's swipe-back is off while either could: a shown area holds it (`swipeBack.ts`) and the server screen turns its stack gesture off.
- **Drags:** on a phone, the sidebar's swipe is off while a plugin channel is in front; the header's sidebar button opens it.
- **A plugin channel has the whole screen:** no tab bar and no voice bar under it, in a call or not; the way back to the call is the sidebar.
- **Activities have their own page** (`ActivityPage`, route `server/[serverKey]/activity/[sessionId]`), shown over the call: the plugin's screen under a header with its name, the call's channel, a way back to the call and its menu (Close, End activity). The call shows a running activity as a tile, and the control bar's Activities button is on, as Mute is, while one runs: pressed then, it offers Open activity (one entry per activity when several run), Change activity (the picker) and Close activity (it ends for everyone, for its starter or who manages the channel). The tile, Open, or starting one from the picker opens its page. Going back keeps the activity open, paused; Close, its end or leaving the call closes it, and the page goes too.
- **The engine is warmed** while a server with plugins is open or this device is in a call, and dropped 60 seconds after.

### Building the Flutter module

`mobile/scripts/build-flutter.sh android|ios [--release]` builds `mobile/flutter_module`:
- **Android:** AARs into `flutter_module/build/host/outputs/repo`. Debug is x86_64 only, for the emulator; release is arm64 and x86_64. The config plugin `mobile/plugins/withPluginArea.ts` adds that repository and Flutter's to the Gradle build.
- **iOS:** unsigned frameworks into `mobile/modules/plugin-area/ios/Frameworks` (the app that embeds them is signed when it's built).

CI's Phone jobs install Flutter and run it before building the app. `PLUGIN_GATES=1` compiles in Stage A's measurements (`flutter_module/lib/src/gates.dart`).

The APK holds only arm64 and x86_64, the processor types the core is built for, and its native libraries are compressed (`useLegacyPackaging`): the Flutter engine adds about 9 MB to the download instead of 18.

### Hand checks for real phones

None of these has been run on a real phone, and **none on an iPhone**. "Emulator" means it was run on the Android emulator against a local server (the plugins M4 ledger, Task 16).

| Check | Android | iPhone |
|---|---|---|
| Open a plugin channel; open its modal; close it with its button and with back | emulator | not run |
| Type in a plugin's text field with the keyboard up: the whole sheet stays above it | emulator | not run |
| The iPhone's swipe-back with a plugin's modal open | — | not run |
| Rotate with a plugin screen open: it redraws and keeps its state | emulator | not run |
| Switch the theme, then the font size, with a plugin screen open | not run (Android restarts the app on a font change) | not run |
| A plugin's image, user, rich text, file picker and upload, link, copy and sound | emulator | not run |
| Start an activity in a call; draw; put the call away and bring it back: the drawing is there | emulator | not run |
| End an activity: `This activity ended`, then the grid | emulator | not run |
| Two phones in one call: the other sees the tile and every stroke; the other ends it | not run | not run |
| A plugin turned off and on: the open channel says it's unavailable, then opens | not run | not run |
| The tablet: the channel list beside a plugin screen; an activity at tablet width | not run | not run |
| A call from another app arrives while an activity is open | not run | not run |
| The app killed in the background, reopened on a plugin channel | not run | not run |
| The members sheet (chat header on phone, 3rd column on tablet): search, presence indicators, role groupings | fake world | not run |
| Member profile: avatar with presence, role chips, member-since date, identity key, copy fingerprint, volume and mute | fake world | not run |
| The server button: header button on Android, search tab bar slot on iOS while inside a server | fake world | not run |
| Server settings sheet: member and online counts, Hosting row ("Open on a computer"), Leave server | fake world | not run |
| Profile page: avatar photo picker and square crop, display name, status picker (Online, Idle, DND) | fake world | not run |
| Voice & audio settings: noise suppression, volume sliders, microphone test with live LevelMeter | fake world | not run |
| Sounds settings: soundboard volume and mute toggle, voice sound cues toggles | fake world | not run |
| Storage and logs: media cache calculation and clear cache, session log export | fake world | not run |
| Voice cue timing: deafening and undeafening each play one sound against a real server | not run | not run |

## Using the app

### Installing it

- **Android 8.0 and up:** download `opencord-android.apk` from a release on GitHub and open it on the phone. Android asks to let your browser or files app install apps; allow it once. A newer APK installs over the old one and keeps your identity and servers. (A release carries the APK once it is signed with Opencord's key: docs/releasing.md, "Phone apps".)
- **iPhone (iOS 16.4 and up):** download `opencord-ios-unsigned.ipa` from a release. It isn't signed, since Opencord isn't on the App Store yet, so install it with a sideloading app such as [AltStore](https://altstore.io) or [Sideloadly](https://sideloadly.io), which signs it with your Apple ID. With a free Apple ID the app has to be signed again every 7 days; the sideloading app can do that for you. TestFlight comes later.

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
  - **Formatting & Spoilers:** markdown formatting is supported: `*italic*`, `**bold**`, `~~strikethrough~~`, `> blockquote`, and `||spoiler||` text that blurs and reveals on tap.
  - **Syntax highlighted code blocks:** fenced code blocks (` ```lang `) render with syntax highlighting across common languages and a one-tap copy button in the top right.
  - **Autocomplete:** typing `@` brings up an autocomplete list of server members (ranked by relevance and nickname); typing `#` autocompletes text and voice channels.
  - **Emojis:** the smiley button opens the emoji picker sheet with Unicode emojis and custom server emojis, organized by categories with quick search and recently used emojis.
  - **Media & Previews:**
    - **Videos:** MP4 and common video attachments show a video player tile with playback duration. Tapping opens full-screen video playback with native controls.
    - **Audio files:** MP3 and audio attachments stream and play directly in the chat row with play/pause, position timer, and an interactive seek scrubber.
    - **Text previews:** text files up to 256 KB offer an in-app text preview modal with line numbers and monospace font.
  - **Text in voice channels:** voice channels with text enabled display a **Chat** button in the call header, opening the channel's chat pane as a sheet over the call on phones and as a side column on tablets.
  - **Reconnecting & Identity changes:**
    - When a server is reconnecting or has failed, a connection banner appears above messages with a **Retry now** button; messages remain visible at 60% opacity with composer disabled.
    - If a server's identity key changes, chat is greyed out and a full-screen **Server identity changed** warning opens showing trusted vs present fingerprints, with buttons to **Disconnect** or destructive **Forget server**.
  - **Invites:** generate instant invite links with custom expiration (1 hour, 1 day, 7 days, Never) and max uses (1, 5, 10, 25, 100, No limit), with quick **Share** via the system share sheet and **Copy** to clipboard.
  - **Switcher / Search:** tap search to find channels, servers, and members by prefix or fuzzy match, instantly navigating on tap.
- **Voice messages:** hold the mic beside the field to record, and let go to send. Slide left to cancel, or slide up to lock the recording and keep both hands free; a locked recording can be stopped and heard before it is sent. Voice messages play in the chat at 1×, 1.5× or 2×.
- **Members:** tap the people button in the chat header (or view the 280 pt right column on tablet width ≥ 1000 pt). Members are grouped with voice participants first (`In this voice channel`), followed by hoisted roles in hierarchy order, then `Online` and `Offline` (collapsed after 30). Tapping a member opens their profile with their avatar, status badge, role chips, `Member since` date, identity key fingerprint with copy button, and local volume and mute controls.
- **The server button and settings:** on Android, tap the three-dots menu button in the header (or tap the server's name in the channel list). On iOS, while inside a server, the tab bar's Search tab transforms into the `Server` tab. Both open the server settings sheet with server icon, name, member and online counts, the Hosting row (`Open on a computer` for the owner, or, with voice on, a member who can manage the server or the music; later plans register additional admin rows), and `Leave server`.

### Voice

- **Join** from a voice channel's page. Mute, deafen, the soundboard and leave are at its foot.
- **While in voice**, a bar shows the channel (and what music plays) above the tab bar, and floats above the field in a chat. Tap it, or pull it up, to open the call, which fills the screen; pull the call down to put it away. While an activity runs, the call's Activities button is on and opens a menu to open, change or close it.
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

- **Your profile:** tap the top card to open the **Profile** page:
  - **Profile picture:** your avatar at 96 pt with **Edit** to take a new square photo or pick one from the library (with crop), or remove it. When updated, the picture uploads to each connected server, catching up offline servers on their next connect.
  - **Status:** choose between **Online** (*Shown as online*), **Idle** (*Away for now*), or **Do not disturb** (*No notifications*). Your status badge updates everywhere and syncs across servers.
  - **Display name:** change your public name across all servers.
  - **Your identity:** view your key's monospace fingerprint with **Copy public key**, and back up your identity key to a passphrase-encrypted file.
- **Voice & audio:**
  - **Noise suppression:** choose between **Off** (*Sends the microphone as it is*), **Standard** (*Removes steady noise; very light*), or **High** (*Also keyboards and dogs; heavier*). On lower-end hardware, if High noise suppression drops frames, it automatically falls back to Standard with a notification.
  - **Volume:** adjust **Input volume** (microphone gain) and **Output volume** sliders from 0 % to 200 %.
  - **Microphone test:** tap **Test** to hear your microphone loopback and observe the live accent-colored **LevelMeter** bar (−60 to 0 dBFS). Audio routing automatically configures for recording, and the test safely stops when joining a call, leaving the page, or backgrounding the app.
- **Sounds:**
  - **Soundboard volume:** control how loud soundboard sounds and previews play (0 % to 100 %), with a toggle to **Mute all soundboards**.
  - **Voice sounds:** toggle individual voice sound cue groups heard only by you: *Joining and leaving*, *Others joining and leaving*, *Mute and unmute*, and *Deafen and undeafen*.
- **Storage and logs:**
  - **Media cache:** calculates the total size of cached thumbnails, pictures and voice messages kept on device. Tap destructive **Clear media cache** (confirmed with a prompt) to wipe cached files while preserving active downloads.
  - **Logs:** **Export this session's log** shares it through the system share sheet (never containing passwords, keys, or messages; the device model shows as `unknown` until the core init signature accepts device metadata). If the previous session ended unexpectedly, an option to **Export the last session's log** is provided.
- **Appearance & Music:** switch theme (**System**, **Light**, or **Dark**); under Music preferences, set your personal music volume, **Mute music**, and **Lower music while people talk**; and view the app and core versions.

### Not there yet

| What | When |
|---|---|
| Camera, screen sharing, watching streams | The next phone plan: it needs the phones' video codecs. |
| Private messages and friends | A later plan. The core has them; the phone needs their screens. |
| Notifications when the app is closed | A later plan, with a push relay on the server. |
| Server administration: managing channels, roles, bans | Later plans (P4 settings). P0 added the server button and sheet, P2 added invites. |
| A list of your own uploaded songs | Later: the server can't list a member's uploads yet. |

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

### iPhone hand checks

Certain features require manual verification on physical iOS hardware:
- **Photo picker and crop:** select a picture from the camera and Photos library, verify the 1:1 square crop interface, and check that the resulting avatar uploads and scales correctly.
- **Microphone test with audio routing:** verify the microphone loopback test works through the built-in speaker/earpiece, and test switching between AirPods or Bluetooth audio devices (audio session activation and switching).
- **High noise suppression on older iPhones:** verify audio performance and frame latency with High noise suppression on older hardware (e.g., iPhone 11), ensuring graceful fallback to Standard if audio threads lag.
- **Voice cue sounds:** confirm Join, Leave, Mute, Unmute, and Deafen cues play audibly with headphones and that Leave does not clip before audio session deactivation.
- **Voice cue timing against a real server:** confirm that deafening and undeafening each play one single sound on both iPhone and Android against a real server, verifying that the 50 ms / 100 ms debounce hold prevents double sounds.
- **Video player & Picture in Picture:** open a video attachment from chat into the full-screen player, verify playback controls and audio track playback, and test Picture-in-Picture transitions when swiping to background.
- **Share sheet with invite:** open the Invite sheet, tap **Share**, and verify the native iOS `UIActivityViewController` opens with the server link and recipient options.
- **Emoji sheet with keyboard interaction:** open the emoji sheet from the composer, verify smooth sheet presentation without layout jumps, and ensure seamless transition when tapping into the text input to raise the iOS keyboard.

### Android's minimum is 8.0

The app needs Android 8.0 (API 26), AAudio's first version, which cpal needs for sound: linking against API 24 fails on `-laaudio`. `expo-build-properties` sets the app's `minSdkVersion` to 26.

### iOS's minimum is 16.4

That is Expo SDK 58's template. `scripts/build-core.sh ios` builds the core for it (`IPHONEOS_DEPLOYMENT_TARGET`). Without that, the C in the core's crates (Opus, ring, SQLite) is compiled for the SDK's own iOS version while Rust links for iOS 10, and the link fails on `___chkstk_darwin`.

React Native is built from source on iOS (`buildReactNativeFromSource` in `app.config.ts`), which makes the first build slower. `@react-native-menu/menu` 2.0.0 imports `"RCTBridge.h"` as a local header, and the precompiled React core doesn't provide it.

The simulator build is for Apple Silicon only: the core's xcframework has `aarch64-apple-ios` and `aarch64-apple-ios-sim` slices, so a simulator build passes `ARCHS=arm64`. (CI builds for phones, where arm64 is the only architecture.) An Intel Mac's simulator would need `x86_64-apple-ios` added to `ios.targets` in `ubrn.config.yaml`.
