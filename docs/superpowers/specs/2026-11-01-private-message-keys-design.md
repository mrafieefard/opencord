# Private message keys, apart from the identity key (design, 2026-11-01)

Status: design, waiting for the owner's approval. Desktop only. It replaces
how devices are trusted in private messages (design 2026-10-22, §10.2), and
nothing else about private messages.

## 1. Why

Today one key does two jobs. A person's **identity key** signs them in to
every server, and it also signs every device that may read their private
messages. The lock button in a private chat shows that same key's
fingerprint. So:

- **Someone who steals the identity key** (it can be exported to a file, and
  imported on another computer) can sign a device of their own. From then on
  every new private message sent to that person, and every copy their own
  devices send themselves, also goes to the thief's device. Nothing in the
  app shows it.
- **The "fingerprint" isn't about the chat.** It's the person's account key.
  Comparing it proves who the person is, not that nobody else is reading.

The owner wants private message keys kept apart from the identity key.
Private keys must never go to a server. Two people can check that their chat
is private by comparing a 4×4 grid of real words. If one key leaks, it
mustn't open the whole chat.

## 2. Who we defend against (the owner's words)

1. **The server owner** trying to read private chats.
2. **Someone who stole a person's identity key.**

## 3. Goals

- Each device has its own private message keys, made on the device. The
  private halves never leave it, and are never sent to any server.
- Neither the identity key nor anything derived from it can make a device
  receive private messages.
- A conversation shows **16 words in a 4×4 grid**. Both people see the same
  words exactly when each one's app encrypts to the same devices for both of
  them. They compare them (in person or on a call) and mark the chat
  verified.
- A device appearing for either person changes the words. The chat says so
  plainly, and a verified chat goes back to unverified.
- A new device of your own receives nothing until one of your existing
  devices approves it, by the two of them showing the same words.
- A key per message stays: every message has its own key (Olm's ratchet, as
  now). One message key opens one message.

## 4. Non-goals

- **The phone app.** It has no private messages yet; it stays that way until
  the owner says otherwise. The server and core changes mustn't break the
  phone app's build or tests.
- History on a new device. A new device starts empty, as now.
- Keeping existing private chats. The app is in beta; old chats, keys and
  waiting messages are removed (§9).
- Changing friends, message requests, blocks, permission slips, relays or
  typing. They don't decide who can read a message. Slips stay signed by the
  identity key, since a server checks them.
- Hiding who talks to whom from the server (it relays, so it knows).
- Backing up private message keys.

## 5. Anti-goals (a failure even if everything works)

- A stolen identity key lets anyone read a private message, or makes a
  device receive one, without a word changing or a notice showing.
- A server can make your app send a message to a device it added to a chat
  you've already started, without a warning and without the words changing.
- A private key, or anything that would let a server compute one, goes to a
  server.
- The two people see the same words while their apps encrypt to different
  devices.
- The words are shown as verified after either person's devices changed.
- Two people compare words, they match, and the chat still says unverified,
  or a match on one device marks a different chat verified.

## 6. Keys

### 6.1 A device's keys

Each device has, as now, a vodozemac Olm account: an **Ed25519 key** (the
device's signing key) and a **Curve25519 key** (for starting sessions),
plus one-time and fallback prekeys signed by the Ed25519 key. These are the
private message keys. They're made on the device, pickled under the local
`pickle` key (keychain-held master key, unchanged), and only their public
halves go to servers.

What changes is who vouches for a device:

- **Today:** the identity key signs `"opencord-device-v1" || device_id ||
  curve25519 || ed25519`. Servers and peers reject devices without that
  signature.
- **New:** the device signs its own keys with its Ed25519 key over
  `"opencord-device-v2" || device_id || curve25519 || ed25519`. This only
  proves the Ed25519 key is held by whoever sent the bundle. Trust comes from
  approvals by the person's other devices (§7) and from the words (§8),
  never from the identity key.
- **Approvals.** When one device approves another of the same person (§7),
  it signs `"opencord-approve-v1" || approver device_id || new device_id ||
  new curve25519 || new ed25519` with its Ed25519 key and uploads it. A
  device's listing carries the approvals it has (`DeviceKeys.approvals`).

### 6.2 What the server still does

A device registers over a signed-in session (`RegisterDevice`), so the
server knows which user a device belongs to and routes by that. The server
checks the self-signature and the lengths, as it checks sizes now. It's
trusted for routing only: the app treats every device list from a server
as a claim, checked by §7 and §8.

## 7. Your own devices

A person can load the same identity on several desktops (identity import).
Each one has its own keys (§6.1).

- **Approved devices.** Each device keeps a local list of its person's
  devices it trusts: itself, plus the ones approved. It sends copies of your
  own messages only to approved devices, and accepts copies only from them.
- **The first device** of a person has nothing to approve it. It's approved
  by itself.
- **A new device.** When a device sees a new device of its own person in a
  server's list, it shows a notice: "A new device signed in as you. Approve it
  only if it's yours." Approving opens the same 4×4 grid on both devices,
  made from the two devices' Ed25519 keys (§8.1, with "opencord-own-words-v1").
  The person checks that both screens show the same words and taps **They
  match** on each. Each device then adds the other to its approved list.
  Each device then adds the other to its approved list, and the old device
  uploads its signed approval of the new one (§6.1) to every server this
  device is registered on (`ApproveDevice`). **Not mine** hides the device
  for good on this device and keeps it out of everything.
- **Trust follows approvals.** A device trusts a device of its own person
  when the device itself approved it, or when the listing carries an
  approval signed by a device it already trusts. So approving a third
  device on either of the first two is enough for all of them.
- **Until approved** a new device is sent no copies of your messages, and
  its copies are ignored. Other people treat it as a device nobody they
  trust approved (§8.2).

The point: a thief with your identity key can register a device, but none of
your devices will send it anything, and nobody you talk to trusts it without
a warning (§8.2). A server can't forge approvals, since it has no device's
private key.

## 8. The words

### 8.1 How they're made

For a conversation between A and B, as seen on one device:

- **A's set:** the Ed25519 keys of A's devices this device encrypts to for
  A (for yourself: your approved devices, this one included).
- **B's set:** the same, for B.
- Each set is sorted (bytes) and hashed: `set_hash = SHA-256("opencord-set-v1"
  || count (u32 BE) || key1 || key2 …)`.
- The two set hashes are sorted, so both sides order them the same way:
  `digest = SHA-512("opencord-words-v1" || lower || higher)`.
- The first 176 bits of `digest`, 11 bits at a time, pick 16 words from the
  BIP-39 English list (2048 words, distinct first four letters). They show
  in a 4×4 grid, read left to right, top to bottom.

So both people see the same 16 words exactly when they agree on both sets.
176 bits is far beyond what anyone can make collide.

The identity key isn't used anywhere in this.

### 8.2 When devices change

- **The first time** you talk to someone, their listed devices are trusted
  as they are (nothing to check against yet). The words are how you check
  them.
- **Later, a new device approved by one of theirs you trust** joins their
  set. The chat shows a divider "*Name*'s devices changed. Compare the
  words again to be sure only you two can read this." (or "Your devices
  changed…"). A verified chat goes back to unverified.
- **A new device nobody you trust approved** (they reinstalled and lost
  every device, or a server or a thief added it) is **held**: it isn't sent
  your messages, and what it sends shows marked "from a device you haven't
  checked". A banner above the composer says "*Name* has a device you
  haven't checked. Your messages won't reach it until you compare the words
  again." with **Compare words** and **Send to it anyway**. Comparing (They
  match) or Send to it anyway adds it to their set, and the divider shows.
  *(Recommendation; see Q1.)*
- **A device is removed** (it expired, or was unregistered): the chat
  shows "*Name* removed a device." A verified chat stays verified, since
  the remaining devices were all compared. The words change; the
  verification is kept against the new set.

### 8.3 Verifying

- The lock button in the chat header opens **End-to-end encryption**:
  - a status line: "Not verified", "Verified" (green) or "Don't match"
    (red);
  - the 16 words in a 4×4 grid;
  - "Compare these words with *Name* in person or on a call. If they're the
    same, only you two can read this chat.";
  - **They match** and **They don't match**.
- **They match** records the verified state: the two set hashes at the
  moment of comparing. The chat is verified while its current sets are the
  recorded ones, or got smaller from them (§8.2).
- **They don't match** marks the chat "Don't match": the header lock turns
  red, a banner says "The words didn't match. Someone may be able to read
  this chat. Don't send anything private.", and sending still works. Comparing
  again (and tapping They match) clears it.
- Verification is per device. Your other devices compare on their own.
- The header lock shows the state: plain (not verified), green check
  (verified), red (don't match or changed since verified).

## 9. Starting fresh

- The server's migration empties `private_devices`, `private_one_time_keys`
  and `private_pending`. Friends, blocks, privacy settings and friend
  requests stay.
- `Ready` gets a new field, `private_messages_v2` (field 22). New servers
  set it and send `private_messages = false`, so old apps stop offering
  private messages there. New apps use private messages only where
  `private_messages_v2` is true, so they never talk to an old server's
  device list.
- On the first start with this change, the app removes the old private
  databases (`<data>/private/<identity hex>.db`) and starts a new one at
  `<data>/private/v2/<identity hex>.db`. It shows once: "Private messages
  now use new encryption. Your old private chats were removed from this
  device." Contacts (friends, requests, blocks) are kept by moving those rows
  across; messages, sessions and keys aren't.
- The old fingerprint (`public_key_fingerprint` for contacts) and its Copy
  button go.

## 10. What changes where

- **`opencord-common::private`:** `device_payload` v2 (§6.1), self-signing
  and `verify_device` against the device's own Ed25519 key; the words
  (`set_hash`, `conversation_words`, `own_device_words`) and the word list.
- **Protocol:** `DeviceKeys.signature` documented as the self-signature;
  `DeviceKeys.approvals` (approver device id + signature); an
  `ApproveDevice` request; `Ready.private_messages_v2`. Proto tests pin the
  new field numbers.
- **Server:** registration checks the self-signature; `ApproveDevice`
  checks that both devices are the caller's and that the signature verifies
  against the approver's key, stores it, and lists it with the device (an
  approval goes when either device does); migration (§9); `Ready` fields.
  The rest of the relay is unchanged.
- **Core (`crates/opencord-core/src/private/`):** device keys signed by the
  device; approved own devices; approvals made, uploaded and checked; the
  sets each conversation encrypts to, per person, with verified state, held
  devices (§8.2) and change events; words for the bridge; outbox fans out to
  the sets; inbox accepts only devices in a set (or, for your own,
  approved), and marks messages from held devices.
- **App:** the encryption dialog (§8.3), the dividers and banners, the
  "new device signed in as you" notice and the approval grid, the
  one-time "old chats removed" notice.
- **Docs:** `docs/private-messages.md` (encryption sections), a decision
  entry in `docs/decisions.md`.

## 11. Testing

- **Common:** the words are the same for both orders of the sets, change when
  any key is added, and are stable (a fixed vector pinned in a test);
  self-signatures verify and reject a changed byte.
- **Core:** a stolen identity on a fresh device gets no copies of your
  messages and is held by everyone you talk to; a device a server adds with
  a made-up approval is held too. An approval on one device is trusted by a
  third. A removed device keeps a chat verified; an added one doesn't. A
  held device starts getting messages after comparing or Send to it
  anyway.
- **Server:** registration with a bad self-signature is refused;
  `ApproveDevice` refuses another user's device and a bad signature, and
  approvals go with their devices; the migration empties the three tables
  and keeps friends; `Ready` fields.
- **App (widget tests):** the dialog shows 16 words in 4 rows; They match
  turns the lock green; a device change shows the divider and resets it;
  the approval notice and grid.
- **By hand (two profiles, as in `docs/private-messages.md`):** compare
  words between Alice and Bob; import Alice's identity on a third profile
  and check Bob's words change and the third profile gets nothing; approve
  it from Alice's first profile and check copies arrive.
- Not testable here: Windows and macOS by hand (CI runs the tests there).

## 12. Open questions for the owner

- **Q1. Devices nobody you trust approved:** hold them in every chat until
  you compare words or tap Send to it anyway (as §8.2 says), hold them only
  in verified chats, or never hold and only show the notice? Holding
  everywhere is what stops a thief or a server owner in chats nobody has
  verified; the cost is that when someone reinstalls and loses every
  device, your messages wait for them until you compare or tap Send anyway.

## 13. Left to the builder

- Table and column names, and how verified state and held devices are
  stored (sealed like other secret columns where they reveal anything).
- How the word list is embedded (the BIP-39 English list is MIT-licensed
  via its reference implementation; check and note the licence).
- Exact layout of the dialog and grid within the existing dialog and theme
  widgets.
- Whether device-list changes reach the chat from `FetchDevices` results,
  the existing device list events, or both.
