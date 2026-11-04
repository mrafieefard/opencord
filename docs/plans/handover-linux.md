# Handover to the Linux session — the 2026-11-01 batch on Windows

The Windows pass for PRs #21–#29 is done.
**[check-windows-report.md](check-windows-report.md) is the record: 44 of 61
boxes pass, 5 partly, 11 open**, and every open one says what blocks it. The
recipes for anything still unchecked are in
[check-windows-by-hand.md](check-windows-by-hand.md).

## Merge the batch first

§C of the report lists the conflicts: nine files. Two are generated —
`app/lib/src/rust/frb_generated.dart` and
`crates/opencord-core/src/frb_generated.rs` — and must be **regenerated, not
hand-merged**: keeping both sides produced 35 `duplicate_named_argument`
errors here. Four need real judgement: `tray_binding.dart`,
`desktop_shell.dart`, `tray_binding_test.dart`, `private_message_list.dart`.

**Six boxes can only be judged after the merge**, because no single build can
drive them: §1's caption buttons and its header drag (both fully proven, but
across #21's and #22's builds separately), §3's two tray routes, §6's viewer
tray box, §8's quit-from-the-tray.

## Four Windows pull requests are open and green

| | |
|---|---|
| #30 | this report |
| #31 | the Win32 clipboard (§5's clipboard box waits on it) |
| #32 | the UI harness |
| #39 | two media-viewer fixes: the close crash, and the light window frame |

## Things that are yours

1. **§4 cannot be tested against the stock test server.** #23 changes
   `handlers/private.rs`, `server.rs`, `db/mod.rs` and `private.proto`. With
   the old server the client greys **Message** out saying *the server doesn't
   support relay messages* — correct refusal, not a bug. Use #23's own
   `opencord-server-windows-x64.zip`. The plan never says this.
2. **§4's device approval never completed**, on #23's server with a clean
   rig: the peer keeps seeing *"A message from keys you haven't accepted."*
   The failing step was **not** isolated, so treat it as unverified rather
   than a known bug. Worth checking first: the **Approve** banner is gated on
   `own.hasChats`, and #23's own fresh-start migration wipes chats — so a
   freshly migrated profile may have no way to approve a second device, with
   nothing on screen saying why.
3. **`range_reader_reads_any_span` is flaky**
   (`crates/opencord-core/tests/attachments.rs:628`): it asserts a read-ahead
   finishes inside a one-second sleep. Wants a branch of its own, beside
   `fix/flaky-reshape-test`.
4. **`Phone (Android)` fails on infrastructure.** Twice on a PowerShell-only
   pull request — once the SDK manager (`cmake;3.22.1`), once
   `No space left on device`. Both passed on a plain re-run. Re-run before
   reading it as a regression.
5. **One Windows bug is unfixed and is a Windows session's, not yours:** the
   reconnect bar's Reconnect button is dead where `InMaximizeButton` in
   `app/windows/runner/window_channel.cpp` claims a fixed top strip that the
   bar now occupies. Noted so nobody duplicates the work.

## Things worth knowing before you start

**Verify a build's contents; do not trust §B's table.** It is right about #22
and wrong about #21, and a pass was recorded against the wrong build here
before it was caught. One command settles it:

```
git merge-base --is-ancestor origin/<pr-branch> origin/<build-branch>
```

**The plan assumes a merged state in more places than it admits**, and it
never says which *server* a section needs — only which client build. §4 needs
#23's server; §6's tray box needs #24 and #25 together; §3's tray steps need
#24 and #27; §8's needs #24 and #29. Worth a pass over `check-windows.md`.

**The media viewer is structurally fragile, and #39 sidesteps rather than
solves it.** The viewer is a second Flutter view on the app's shared engine,
and the embedder exports no way to remove a secondary view:
`FlutterDesktopViewControllerDestroy` shuts the engine down, which is what
killed the app on every close. #39 stops destroying it at all — close hides,
reopen shows — so the view lives for the process's lifetime. If Flutter ever
ships a real `EngineDestroyViewController`, that is the thing to adopt.

**A person beats a script for anything input-related.** Three findings came
only from the owner at the keyboard — Esc working after a reopen, the white
window border, and "there is no close button on the private page". The
scripted harness produced *false negatives* on all three.

## The one to take most seriously

Three diagnoses in this report were wrong and had to be retracted: a
re-entrancy theory for the crash, "Win32 eats the keys", and "the media
viewer takes no keyboard at all". Each came from reasoning about the code
instead of measuring the running thing, and each read convincingly. What
killed them was cheap: `GetGUIThreadInfo` for focus, the crash log instead of
window counts, and asking a person to press a key.

**If a finding here is not backed by a number, a log line or a human, treat
it as a hypothesis.** The report tries to say which is which.

## The rig, if you need it again

- Server: #23's build, `C:\oc\server-pmk\`, data in `C:\oc\test-server-pmk\`,
  on `localhost:7710`.
- Profiles: `ann` (owner) and `ben` (display name "John").
- Parked, recoverable: the earlier `alice`/`bob`/`alice2` profiles in
  `…\Opencord\profiles\_old-0026`, and the pre-#23 server database in
  `C:\oc\test-server-pmk\data-old-0026`.
- Test material: `C:\oc\material\`.
