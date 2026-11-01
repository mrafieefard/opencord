# Private message keys, apart from the identity key (design, 2026-11-01)

Status: approved by the owner 2026-11-01, then revised before building (§13
says what changed and why). Desktop only. It replaces how devices are
trusted in private messages (design 2026-10-22, §10.2), and nothing else
about private messages.

## 1. Why

Today one key does two jobs. A person's **identity key** signs them in to
every server, and it also signs every device that may read their private
messages. The lock button in a private chat shows that same key's
fingerprint. So:

- **Someone who steals the identity key** (it can be exported to a file and
  imported on another computer) can sign a device of their own. From then on
  every new private message sent to that person, and every copy their own
  devices send themselves, also goes to the thief's device. Nothing in the
  app shows it.
- **The "fingerprint" isn't about the chat.** It's the person's account key.

The owner wants private message keys kept apart from the identity key,
private keys never sent to a server, two people able to check their chat by
comparing a 4×4 grid of real words, and no single leaked key opening the
whole chat.

## 2. Who we defend against (the owner's words)

1. **The server owner** trying to read private chats.
2. **Someone who stole a person's identity key.**

## 3. Goals

- Each person has a **chat key**, made on their first device and kept only
  on their own devices. It has nothing to do with the identity key. It
  signs that person's devices; only devices it signed get their messages.
- No private key ever reaches a server in a form the server can read. The
  chat key goes from one of your devices to another only inside an Olm
  message, after you approved the new device by comparing words on both.
- A conversation shows **16 words in a 4×4 grid**, made from both people's
  chat keys. Both see the same words exactly when each app holds the other's
  real chat key. They compare them (in person or on a call) and mark the
  chat verified.
- When someone's chat key changes, the chat says so plainly, nothing goes to
  the new key until the other person compares words or accepts it, and a
  verified chat goes back to unverified.
- A key per message stays: every message has its own key (Olm's ratchet, as
  now). One message key opens one message.

## 4. Non-goals

- **The phone app.** It has no private messages yet; it stays that way until
  the owner says otherwise. Server and core changes mustn't break the phone
  app's build or tests.
- History on a new device. A new device starts empty, as now.
- Keeping existing private chats. The app is in beta: old chats, keys and
  waiting messages are removed (§10).
- Changing friends, message requests, blocks, permission slips, relays or
  typing. Slips stay signed by the identity key, since a server checks them;
  they decide who may write, not who can read.
- Hiding who talks to whom from the server.
- Backing up the chat key. Losing every device means a new chat key, which
  your contacts see as a key change.

## 5. Anti-goals (a failure even if everything works)

- A stolen identity key lets anyone read a private message, or makes any
  device receive one, without the other person seeing a key change.
- A server can make your app send a message to a device it made up, without
  a key change showing.
- A private key, or anything a server could compute one from, reaches a
  server.
- Two people see the same words while one app holds a different chat key
  for the other than the one they really have.
- A chat shows verified after either person's chat key changed.
- A message is sent to a new chat key before its banner was shown and
  answered.

## 6. Keys

### 6.1 The chat key

- An Ed25519 key pair, made on a person's first device the first time
  private messages open there. Stored in the private database (§10),
  sealed under the local seal key like other secret columns.
- Its public half goes out in every one of that person's device listings
  (`DeviceKeys.chat_key`). Its private half goes nowhere but §7.2.

### 6.2 Device keys

Each device has, as now, a vodozemac Olm account: an Ed25519 key, a
Curve25519 key, one-time and fallback prekeys signed by the Ed25519 key.

- **Today:** the identity key signs `"opencord-device-v1" || device_id ||
  curve25519 || ed25519`.
- **New:** the chat key signs `"opencord-device-v2" || device_id ||
  curve25519 || ed25519` (98 bytes). `DeviceKeys` carries `chat_key` (32
  bytes) and `signature` (64, by the chat key).
- A server checks the signature against the listing's own `chat_key` and
  routes by the signed-in user. It can't tell a real chat key from one it
  made up; apps decide which chat key they trust (§8).

## 7. Your own devices

### 7.1 A new device

A device that opens private messages and has no chat key makes one at once
and registers with it. Then:

- **It's the only one** of your devices listed anywhere it's connected: it's
  your first device. Nothing to do.
- **Your other devices use a different chat key.** A device with no private
  chats yet (no messages stored) shows, in the home view: "Your other
  devices use different keys. Approve this device on one of them so they
  share your chats." with **Keep new keys** (hides the banner on this
  device). A device with chats shows, for each such device: "A device signed
  in as you with different keys. Approve it only if it's yours." with
  **Approve** and **Not mine**. Approving gives the other device this
  device's chat key. *(Builder's call: which side is "new" is decided by
  whether it has chats, since devices can't compare ages.)*

### 7.2 Approving

1. On the old device, **Approve** opens a 4×4 grid made from the two
   devices' Ed25519 keys (§9.2). The person checks the new device shows the
   same grid and taps **They match**. The old device sends the new device a
   `ChatKeyShare` (the chat key's 32-byte private half) inside an Olm
   message, through any server both devices are registered on.
2. When the share arrives, the new device shows "*Your device* wants to give
   this device its keys." with the same grid (from the two Ed25519 keys) and
   **They match** / **They don't match**. They match: the new device replaces
   its chat key with the shared one, signs its device keys again and
   registers again on every server. They don't match: the share is thrown
   away.
3. A share is accepted only from a device listed for yourself, and only
   after this device's own They match for that sender. A share that arrives
   before the tap waits, sealed.

**Not mine** hides that device on this device: no notice, and nothing is
ever sent to it.

### 7.3 Copies of your own messages

Your messages are copied only to your devices signed by your chat key, and
copies are accepted only from them. A thief with your identity key can
register a device, but it has its own chat key, so it gets nothing and your
devices warn you (§7.1).

## 8. Other people

### 8.1 Whose chat key you trust

- Each contact has one **pinned chat key** on this device.
- **First contact:** the chat key on the most of their listed devices is
  pinned (ties: the lowest key, by bytes). The words are how you check it.
- A device of theirs is trusted when the pinned key signed it. Only trusted
  devices are sent your messages; messages from any other device show as "a
  message from keys you haven't accepted" (the existing undecryptable line,
  with this text).

### 8.2 When their chat key changes

A listed device of theirs is signed by a chat key other than the pinned one
(they reinstalled and lost every device, or a server or a thief made it up):

- The chat shows a banner above the composer: "*Name*'s keys changed. They
  may have reinstalled, or someone may be trying to read this chat. Your
  messages won't reach the new keys until you compare the words." with
  **Compare words** and **Use new keys anyway**.
- **Compare words** opens the dialog (§9.3) with the words made from the new
  key. They match pins it and marks the chat verified. **Use new keys
  anyway** pins it, unverified. Either way a divider shows "*Name*'s keys
  changed." and devices signed by the old key get nothing more.
- Until then, their devices signed by the pinned key still get your
  messages. If none is listed any more, the composer is disabled and the
  banner stays: nothing you write would reach them.
- **Decided (owner, Q1):** this holds in every chat, verified or not.
- More than one other key: the banner offers the one on the most devices.

When your **own** chat key changes (§7.2), every chat on this device shows
"Your keys changed." and goes back to unverified.

A new device of theirs signed by the pinned key needed their approval on
another of their devices, so it's trusted without asking. The chat shows a
quiet divider "*Name* signed in on a new device." and stays as verified as
it was.

## 9. The words

### 9.1 A chat's words

`digest = SHA-512("opencord-words-v1" || lower || higher)`, where lower and
higher are the two chat keys (yours and their pinned one) sorted by bytes.
The first 176 bits of `digest`, 11 bits at a time (most significant bit
first), pick 16 words from the BIP-39 English list (2048 words). They show
in a 4×4 grid, read left to right, top to bottom. Both people get the same
16 words exactly when they hold the same two keys. The identity key isn't
used.

### 9.2 Pairing words

The same, with `"opencord-own-words-v1"` and the two devices' Ed25519 keys.

### 9.3 Verifying

- The lock button in the chat header opens **End-to-end encryption**:
  - a status line: "Not verified", "Verified" (green) or "Didn't match" (red);
  - the 16 words in a 4×4 grid;
  - "Compare these words with *Name* in person or on a call. If they're the
    same, only you two can read this chat.";
  - **They match** and **They don't match**.
- They match records the chat as verified for these two keys. It stays
  verified while neither key changes.
- They don't match marks the chat "Didn't match": the lock turns red, a
  banner says "The words didn't match. Someone may be able to read this
  chat. Don't send anything private.", and sending still works. They match
  later clears it.
- Verification is per device.
- The header lock: plain (not verified), green (verified), red (didn't
  match). The tooltip says which.

## 10. Starting fresh

- **Server** migration: empties `private_pending`, `private_one_time_keys`
  and `private_devices`, and adds `private_devices.chat_key`. Friends,
  blocks, privacy settings and friend requests stay.
- **Ready** gets `private_messages_v2` (field 23). New servers set it and
  send `private_messages = false`, so old apps stop offering private
  messages there. New apps use only servers with `private_messages_v2`.
- **SendPrivateMessage** gets `withheld` (field 5): the recipient's and
  caller's devices the app deliberately doesn't encrypt for (§8). The server
  accepts a send when the copies and the withheld devices together are
  exactly the devices it lists, with none in both; otherwise
  `DEVICE_LIST_CHANGED`, as now.
- **The app's database** is migrated in place: the device, its sessions,
  the known devices, messages and received-event records are deleted, and
  contacts lose their conversation, relay, activity and read state. Friends,
  requests, blocks, names, mutes and privacy settings stay. The app shows
  once: "Private messages now use new encryption. Your old private chats were
  removed from this device."
- The old fingerprint and its Copy button go.

## 11. Testing

- **Common:** words are the same for both key orders, differ for another
  key, and match a pinned vector; pairing words differ from chat words for
  the same keys; v2 device signatures verify only against their chat key.
- **Server:** a bad chat key signature is refused; a send with withheld
  devices goes through, one missing a device or naming a device twice is
  `DEVICE_LIST_CHANGED`; the migration empties the three tables and keeps
  friends; Ready's fields.
- **Core:** first contact pins the majority key; a device with another key
  is held, gets no copy, and its message shows as unaccepted; Use new keys
  anyway and They match pin it; with only held devices sending is refused; a
  stolen identity on a fresh device gets no copies; approving shares the key
  and the new device's copies flow both ways after it re-registers; a share
  is ignored before They match and from someone else; words come out the
  same on both sides.
- **App (widget):** the dialog shows 16 words in 4 rows; They match turns the
  lock green; a key change shows the banner, disables the composer when
  nothing is reachable; the own-device notice and the approval grid.
- **By hand:** two profiles (as in `docs/private-messages.md`) compare words;
  a third profile with Alice's imported identity gets nothing, Bob sees no
  change, Alice sees the notice; approving it from Alice's first profile
  makes copies arrive there.
- Not testable here: Windows and macOS by hand (CI runs the tests there).

## 12. Left to the builder

- Table and column names in the app's database.
- The word list file: the BIP-39 English list (`bitcoin/bips`,
  `bip-0039/english.txt`, 2048 lines, `abandon` … `zoo`); note its origin in
  the file's header comment and in `docs/private-messages.md`.
- Dialog, grid and banner layout within the existing dialog and theme
  widgets.

## 13. Changed after approval (2026-11-01)

The approved version gave every device its own trust: a person's devices
approved each other, and the words covered every device of both people.
Reading it as the builder showed a flaw: each server lists only the devices
that joined that server, so two people's apps would often see different
device sets, and the words wouldn't match with nobody attacking. One chat
key per person keeps everything the owner asked for (separate from the
identity key, never on a server, words, held keys, approval of new devices
by comparing words) and the words only change when someone's chat key
really does.
