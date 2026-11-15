# Phone P2: Chat, the Rest of It — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chat on the phone does what it does on the desktop, voice channels' text chat included: `@`, `#` and `:` autocomplete with mentions sent as tokens, an emoji picker, videos, audio files and text files opening in place, highlighted code, a reconnecting banner, inviting people, a clear screen when a server's identity changes, and a Search that finds channels and people.

**Architecture:**
- **Ports, not rewrites.** The desktop's mention rules (`app/lib/features/chat/mentions.dart`), suggestions (`suggestions.dart`) and switcher results (`app/lib/features/switcher/switcher_results.dart`) are pure functions with tests. Each becomes a TypeScript module with the desktop's test cases ported one for one.
- **Emoji come from the desktop's generator.** `app/tool/build_emoji.dart` also writes `mobile/src/features/emoji/emojiData.json`, so the two apps offer the same emoji with the same names.
- **Media plays through the system** (design §7.4): videos with `expo-video` from the attachment cache, audio files with `expo-audio`, which the app already uses for voice messages.

**Tech Stack:** as P0. **New dependencies** (Claude's calls, for the owner to check): `expo-video` (~58, Expo's own) and `highlight.js` 11 (the grammar family `re_highlight` ports on the desktop, so the colours match; only the desktop's languages are registered).

**Spec:** design §3 (P2), §6.3, §7.4. Desktop references are named per task.

## Global Constraints

- **Branch:** `feat/phone-chat`, from `feat/phone-you` (P1). Draft PR into `beta` at Stage A's end, stacked.
- **Scope:** `crates/opencord-mobile/`, `mobile/`, `app/tool/build_emoji.dart` (an extra output only; the desktop's generated file must not change, which `git diff app/lib` proves), `docs/`.
- **Mentions:** messages carry `<@id>` and `<#id>`; the composer shows `@Name` and `#name`. Sending encodes; editing a sent message decodes. Text inside code stays as written. That's the desktop's rule and the phone must match it, or the desktop sees raw names.
- P0's Global Constraints from "Ids are decimal strings" on apply.
- **Numbers follow the facade's rule** (`crates/opencord-mobile/src/types.rs:1-3`): ids are decimal strings; times (ms since the epoch), sizes, durations, positions and counters cross as `f64` through `convert::number` and `convert::unsigned`; a draft id comes back through `convert::draft_id`. No `u64` or `i64` field in a record JavaScript reads, whatever the core's type: a `u64` becomes a JavaScript `bigint` and breaks every comparison.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p2/progress.md`.

| Task | Score | Owner |
|---|---|---|
| 1 Facade: invites | 2 | Gemini |
| 2 Mentions and the active token | 3 | Gemini (focus: names with spaces, code spans, the caret) |
| 3 Emoji data and the picker | 2 | Gemini |
| 4 Composer suggestions | 3 | Gemini (focus: the list and the keyboard, the command list sharing the space) |
| 5 Videos, audio files and text previews | 3 | Gemini (focus: a file still downloading; memory with many videos in the list) |
| 6 Highlighted code | 2 | Gemini |
| 7 The reconnecting banner and a changed identity | 3 | Gemini (focus: the cached chat greyed, not cleared) |
| 8 Invite people | 2 | Gemini |
| 9 Search finds channels and people | 3 | Gemini (focus: ranking parity with the desktop) |
| 10 Text chat in voice channels | 3 | Gemini (focus: unread counts for a voice channel's chat) |
| 11 Live check, docs, D-entry, PR | 2 | Gemini |

Stages: A = Tasks 1–4 (the composer), B = 5–10 with the review, C = 11. Commands as P0.

## Review Focus

1. **A mention of someone whose name changed, or who left,** between typing and sending. Expected: the name typed is encoded if it still matches a member at send time; if not, it's sent as plain text, exactly as `encodeMentions` does. Pinned in Task 2 by the ported `mentions_test.dart`.
2. **Editing an old message with mentions.** Expected: the field shows `@Name`, and saving sends the tokens back unchanged when untouched. Pinned in Task 2: `decode then encode is the same message`.
3. **A video that's still downloading, or fails.** Expected: the tile shows progress over its poster and a retry on failure; it never plays a half-written file (the facade writes to a `.part` file first). Pinned in Task 5: `a video plays only from a whole file`.
4. **Many videos in one channel.** Expected: at most one `VideoView` exists at a time (the one playing); the rest are posters, so the list doesn't hold a decoder per row. Pinned in Task 5: `only the playing video has a player`.
5. **A server whose identity changed while its chat is open.** Expected: the chat stays readable, greyed, with the banner; the composer is disabled; the full-screen explanation opens once, not on every reconnect attempt. Pinned in Task 7: `the identity screen opens once per change`.

---

## Stage A — The composer

### Task 1: Invites in the facade

**Files:** create `crates/opencord-mobile/src/api/invites.rs`; modify `src/api/mod.rs`, `src/types.rs`, `src/convert.rs`.

**Interfaces (produces):** `InviteRecord { code, link, web_link: Option<String>, domain_id: Option<String>, created_by: Option<String>, created_at_ms: f64, max_uses: Option<u32>, uses: u32, expires_at_ms: Option<f64> }`; `invite_create(server_key, max_uses: Option<u32>, expires_in_s: Option<u32>) -> Result<InviteRecord>` (domain `None`: the server's default, as the desktop's first choice); `invites_fetch(server_key) -> Result<Vec<InviteRecord>>` and `invite_revoke(server_key, code)` for P4.

- [ ] Tests `an_invite_crosses_with_its_link`, `ids_in_an_invite_are_strings`; run (fail), implement, run (pass); clippy; bindings; `CoreApi`, `NativeCore`, `FakeCore` (the fake returns `opencord://fake.invite/AbC123` and keeps a list); commit `feat(mobile): invites in the facade`.

### Task 2: Mentions and the active token

**Files:** create `mobile/src/features/chat/mentions.ts`; test `mobile/src/features/chat/__tests__/mentions.test.ts`.

**Interfaces (produces), ports of `app/lib/features/chat/mentions.dart`:**
- `encodeMentions(text: string, members: Map<string, string>, channels: Map<string, string>): string` (id → shown name)
- `decodeMentions(content: string, members: Map<string, string>, channels: Map<string, string>): string`
- `activeToken(text: string, caret: number): { trigger: '@' | '#' | ':', query: string, start: number } | null`
- The same private rules: outside code only; longest name first; whole words; `:` needs two letters.

- [ ] **Step 1:** port every test in `app/test/unit/chat/mentions_test.dart`, same names, same strings, plus `decode then encode is the same message`.
- [ ] **Step 2:** run (fail), implement, run (pass).
- [ ] **Step 3:** wire it: `Composer.tsx` sends `encodeMentions(...)`; `EditingBar.tsx` loads `decodeMentions(...)`. Commit `feat(mobile): mentions sent as tokens, as the desktop does`.

### Task 3: Emoji data and the picker

**Files:** modify `app/tool/build_emoji.dart` (a second output); create `mobile/src/features/emoji/emojiData.json` (generated), `emoji.ts` (`searchEmoji(query, limit)`, a port of `app/lib/ui/emoji/emoji.dart`'s), `EmojiPicker.tsx`; modify `Composer.tsx` (a smiley button left of the field); tests `features/emoji/__tests__/emoji.test.ts`, `__tests__/EmojiPicker.test.tsx`.

- JSON shape: `[{ "e": "😀", "n": "grinning face", "s": ["grinning"], "c": "smileys" }, …]`, in the desktop's order; the generator writes both files from the same source in one run. Ledger the command and that `git diff app/lib/ui/emoji` is empty.
- **The picker** is a sheet at half height: a search field, category tabs as icons (the desktop's categories), a grid of 8 columns, recent emoji first (kept in prefs, at most 24). A tap inserts at the caret and keeps the sheet open; the keyboard comes back when it closes.
- [ ] Tests: port the cases of `app/test/unit/emoji_test.dart`; `a tap inserts at the caret`; `recent emoji come first`. Run (fail), implement, run (pass); screenshots; commit `feat(mobile): an emoji picker`.

### Task 4: Composer suggestions

**Files:** create `mobile/src/features/chat/suggestions.ts` (a port of `suggest` in `suggestions.dart:95-135`), `SuggestionList.tsx`; modify `Composer.tsx`, `features/chat/dock.ts` (the list's place above the field, shared with `CommandList`: commands win while the text starts with `/`); tests `__tests__/suggestions.test.ts`, `__tests__/SuggestionList.test.tsx`.

- `@` lists the members who can see the channel (a member's channel access isn't known on the phone; use every member and write a ruling if P0's data can't narrow it), with their highest role's name; `#` lists the text-like channels the reader can view; `:` lists emoji. At most `maxSuggestions` = 8, the desktop's.
- A tap replaces the token with `@Name ` / `#name ` / the emoji and a space.
- [ ] Port `app/test/unit/chat/suggestions_test.dart`; add `commands win while the text starts with a slash`, `a tap replaces the token`. Run (fail), implement, run (pass); screenshots with the keyboard up; commit `feat(mobile): @, # and : suggestions in the composer`. Stage A's end.

## Stage B — Media, states and finding things

### Task 5: Videos, audio files and text previews

**Files:** create `mobile/src/features/chat/attachments/VideoAttachment.tsx`, `AudioFileAttachment.tsx`, `TextPreview.tsx` (a route `app/text-preview.tsx`), `previewable.ts`; modify `AttachmentView.tsx`, `FileAttachment.tsx`; tests in `attachments/__tests__/`.

- **Video:** a poster (the attachment's thumbnail when the server made one, else a dark tile with the duration) with a play button; a tap fetches with `attachmentFile` (progress over the poster), then opens a full-screen `VideoView` with native controls and picture in picture allowed. Inline playback in the list: Claude's call, no (one decoder at a time, Review Focus 4).
- **Audio file** (mp3, ogg, m4a, wav, flac, not a voice message): a row with play, a scrubber and the time, like the voice message player, through `expo-audio`.
- **Text preview:** offered for the desktop's `textPreviewExtensions` (`app/lib/features/chat/attachments/file_icon.dart:7`) up to `maxPreviewBytes` = 256 KB; a `Preview` action beside save. Opens a full-screen monospace scroll view, selectable, with the file name as title and share on the right.
- [ ] Tests: `a video plays only from a whole file`, `only the playing video has a player`, `preview is offered only for text up to 256 KB`, `an audio file plays and seeks`. Run (fail), implement, run (pass); on the emulator play a real MP4 and an MP3; screenshots; commit `feat(mobile): videos, audio files and text previews in the chat`.

### Task 6: Highlighted code

**Files:** create `mobile/src/features/chat/codeHighlight.ts` (registers exactly the languages `app/lib/features/chat/code_highlight.dart` imports, mapping each to its theme colours from the phone's tokens); modify `MarkdownText.tsx` (fenced blocks with a language); test `__tests__/codeHighlight.test.ts`.

- A block over 400 lines or 20 KB isn't highlighted (plain mono), so a pasted log doesn't stall the list. Highlighting runs once per message and is memoised by message id and edit time.
- [ ] Tests: `a rust block gets keyword spans`, `an unknown language stays plain`, `a huge block stays plain`. Run (fail), implement, run (pass); screenshots in both themes; commit `feat(mobile): highlighted code blocks`.

### Task 7: The reconnecting banner and a changed identity

**Files:** create `mobile/src/features/chat/ConnectionBanner.tsx`, `mobile/src/features/servers/IdentityChangedScreen.tsx` (a route `app/server/[serverKey]/identity-changed.tsx`); modify `ChatPane.tsx`, `ChannelList.tsx`, `state/selectors.ts` (`useChatStale(serverKey)`).

- **The banner** (desktop `connection_banner.dart`) above the messages while the server is reconnecting or failed (other than a changed identity): the connection text the servers list already shows (`connectionText.ts`), and `Retry now` (`serverRetryNow`). The messages stay, at 60 % opacity; the composer is disabled.
- **A changed identity** (`ConnectionRecord.Failed` with reason `FingerprintChanged`): a full-screen page, the desktop's words: title `Server identity changed`; body `<name> no longer shows the identity you trusted. Someone may be pretending to be it, or its owner replaced its certificate. Opencord will not connect until you know which: ask the owner, then forget the server and add it again.`; two fingerprints in mono under `Trusted fingerprint` and `Fingerprint shown now` (`Not known` when missing); buttons `Forget server` (destructive, `serverRemove`) and `Disconnect` (closes the page; the server stays failed). It opens once per change (keyed by the presented fingerprint) when the server is opened, and from a tap on the banner.
- [ ] Tests: `the cached chat stays greyed while reconnecting`, `retry now retries`, `the identity screen opens once per change`, `forget removes the server`. Run (fail), implement, run (pass); screenshots (stop the local server to see the banner); commit `feat(mobile): the reconnecting banner and a changed server identity`.

### Task 8: Invite people

**Files:** create `mobile/src/features/invites/InviteSheet.tsx`, route `app/server/[serverKey]/invite.tsx`; modify `features/serverMenu/serverRows.ts` (register `Invite people`, shown with `create_invite` or `manage_server`), `ChannelListHeader.tsx` (a person-plus button for those who may).

- The desktop's words (`app/lib/features/dialogs/invite_dialog.dart`): title `Invite people to <name>`; `Making a link…` while it's made; the link in mono; `Anyone with this link can join <name>.`; `For a browser: <web>` when there's a web link; the trust footnote (`The link carries this server’s fingerprint, so whoever opens it connects to this server and not to an impostor.`, or the domain variant when `domainId` is set); `Expire after` (1 hour, 1 day, 7 days (default), Never) and `Max uses` (No limit (default), 1 use, 5 uses, 10 uses, 25 uses, 100 uses) as choice rows; `Make a new link`.
- **Phone-first:** the main button is `Share` (the system share sheet with the link), and `Copy` (toast `Invite link copied`) beside it. Changing a choice makes a new link.
- [ ] Tests: `the defaults are 7 days and no limit`, `changing a choice makes a new link`, `share passes the link`. Run (fail), implement, run (pass); screenshots; commit `feat(mobile): invite people`.

### Task 9: Search finds channels and people

**Files:** create `mobile/src/features/search/searchResults.ts` (a port of `switcher_results.dart`), `SearchResults.tsx`; modify `app/(tabs)/search/index.tsx` (iOS), `features/servers/ServerListScreen.tsx` (Android: the header's search field shows the same results).

- Results as the desktop's switcher: channels the user can see on every server, the servers, and the members of the last opened server, ranked by the desktop's rules. A tap goes there: a channel opens its server and the chat; a member opens their profile (P0).
- [ ] Port `app/test/unit/switcher/switcher_results_test.dart`; add `a channel result opens the chat`. Run (fail), implement, run (pass); screenshots; commit `feat(mobile): search finds channels and people`.

### Task 10: Text chat in voice channels

**Files:** modify `mobile/src/features/voice/VoicePane.tsx` (a `Chat` button in the call's header while the channel has `textInVoice`), `features/server/VoiceChannelRow.tsx` (the unread dot and mention count, as text channels), `features/chat/ChatPane.tsx` (opens on a voice channel's id); the desktop's reference is `app/lib/features/voice/voice_chat_slot.dart`.

- With `textInVoice` on, a voice channel's page and the open call show a `Chat` button; it opens the same `ChatPane` as a text channel, as a sheet over the call on the phone and beside it on the tablet. The rules for who may read and send are the channel's (P0's access).
- [ ] Tests: `a voice channel with text opens its chat`, `its unread count shows on the row`, `without textInVoice there's no button`. Implement; screenshots; commit `feat(mobile): text chat in voice channels`. Stage B's end and review.

## Stage C — Live and docs

### Task 11: Live check, docs, D-entry and the pull request

- [ ] With the desktop in the same server: mention someone from the phone (the desktop shows a mention and counts it); edit it; send a video from the desktop and play it on the phone; make an invite on the phone and join from the desktop with it; restart the server with a new certificate and see the identity page.
- [ ] `docs/mobile.md` "Chat" and "Not there yet"; the iPhone hand checks (the video player, picture in picture, the share sheet with an invite, the emoji sheet with the keyboard).
- [ ] The next D-number: system players instead of the core's, inline video off, highlight.js. From `git diff`.
- [ ] `.github/check.sh`; push; PR description; `gh pr checks` green; review round; report.
