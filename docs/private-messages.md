# Private messages, friends and offline delivery

This document explains how Opencord implements end-to-end encrypted private messaging, friends, and offline message buffering, for both users and server administrators.

## Overview and architecture

Opencord has no central server. Private messages and friend requests travel through shared servers where both participants are members. Servers act solely as untrusted relays and offline message mailboxes: they route encrypted payloads and store them only until the recipient's devices fetch and acknowledge them.

```
Device A ────(encrypted with Olm)────► Shared Server ────(encrypted with Olm)────► Device B
(Alice)                                (Relay)                                     (Bob)
```

## Encryption and security

### End-to-end encryption with Olm

All message bodies, edits, deletions, relay move notifications, and acceptance signals are end-to-end encrypted using the Olm cryptographic ratchet via `vodozemac` 0.11.1 (`SessionConfig::version_1()`):
- **A key per message:** Every message is encrypted under a unique ephemeral message key derived through the Double Ratchet algorithm, providing forward secrecy and post-compromise security.
- **Multi-device fan-out:** A sender encrypts an outgoing message individually for every verified device belonging to the recipient, as well as for each of the sender's other registered devices.
- **Session management:** Clients maintain up to 5 active Olm sessions per peer identity key (`PeerSessions`), pruning oldest inactive sessions.

### Identity and device verification

- **Root identity:** Each user is identified across servers by their root Ed25519 identity key.
- **Device keys:** Each device generates its own Curve25519 (for Olm Diffie-Hellman exchanges) and Ed25519 (for signing prekeys) key pair.
- **Device signatures:** When registering, the device presents a 64-byte Ed25519 signature from the user's root identity key over `device_payload` (`"opencord-device-v1"` || `device_id` || `curve25519` || `ed25519`). Clients reject any device key bundle not signed by the peer's root identity.
- **Prekeys:** Devices upload one-time Curve25519 prekeys and a fallback prekey, signed by the device's Ed25519 key over `prekey_payload` (`"opencord-prekey-v1"` || `device_id` || `key` || `fallback`). Fallback prekeys are rotated every 30 days (`FALLBACK_ROTATION_MS`).

### What servers see, and what they cannot do

- **What servers see:**
  - Who messages whom and timestamps of delivery.
  - Message ciphertext sizes (up to 32,768 bytes).
  - The number of registered devices per member.
  - Friend requests, answers, cancellations, and friend removals.
  - User privacy settings and member block lists on that server.
  - On a server that relays only between friends, each member's friends among its members.
  - Typing signals and 64-byte permission slips.
- **What servers cannot do:**
  - Read message content, edits, deletions, or relay move notices.
  - Forge messages from a device or alter message payloads without detection.
  - Register rogue devices on behalf of a user (the device must be signed by the user's root identity key).
  - Impersonate a user or fabricate friendships.
- **What servers can do:**
  - Servers can drop, delay, or reorder deliveries, or refuse to relay.
  - Clients protect against replay and duplicate delivery: re-delivered events or duplicate Olm messages are recognized and acknowledged without showing duplicate messages or false decryption errors.

## Relay servers and moving a chat

Private conversations are routed through a mutually shared server where private messages are enabled (`Ready.private_messages` is `true`).

- **Relay selection:** A conversation starts on the server where the interaction began (e.g. from a user profile or where a message request arrived).
- **The relay chip:** The chat header displays the current relay server. Clicking the relay chip lists mutual servers shared by both users.
- **Moving a chat:** Either user can switch the relay to another mutual server. The mover's client sends an encrypted `RelayMoved` payload through the old relay, updates its local relay setting, and inserts a wavy `RelayDivider` with a clickable server link.
- **Move too:** The recipient sees a `MoveCard` in chat notifying them of the move with a **Move too** button. Clicking "Move too" switches the recipient's outgoing relay to match, provided this device has joined that server.

## Server settings

Owners and anyone with **Manage server** choose in **Server settings → Private messages**:
- **Private messages:** whether this server relays them at all. Off, it refuses private requests with `PRIVATE_MESSAGES_UNAVAILABLE`; apps stop offering it as a relay. `[private_messages] enabled = false` in the config file overrides this switch, which then can't be turned on.
- **Only between friends:** messages and typing go only between two members who each list the other as a friend (`FRIENDS_ONLY` otherwise). Friend requests still go through, so people can become friends. Apps send their friend lists (`UpdatePrivacy.friend_user_ids`) only to servers with this on, and again whenever a friendship changes; a request accepted through the server, or a friend removed through it, counts at once. Turning it off forgets every list.

## Offline delivery and retention limits

Servers store undelivered events in `private_pending` until the recipient device connects and acknowledges them with `AckPendingEvents`.

### Server limits (`[private_messages]`)

Server administrators configure retention in `opencord.toml`:

```toml
[private_messages]
# Relay end-to-end encrypted private messages and friend requests.
enabled = true

# Days an undelivered event waits for an offline device (1 to 365).
undelivered_days = 30

# Maximum pending events buffered per device (100 to 100000). Drops oldest.
max_pending_per_device = 5000

# Days an inactive device may remain before being purged (7 to 365).
device_expiry_days = 60
```

Environment variable overrides:
- `OPENCORD_PRIVATE_ENABLED`: `true` or `false`
- `OPENCORD_PRIVATE_UNDELIVERED_DAYS`: `1` to `365`
- `OPENCORD_PRIVATE_MAX_PENDING_PER_DEVICE`: `100` to `100000`
- `OPENCORD_PRIVATE_DEVICE_EXPIRY_DAYS`: `7` to `365`

### Device and history constraints

- **10 devices per user:** A user may register up to 10 devices (`MAX_DEVICES_PER_USER`). Registering an 11th device evicts the device unseen for the longest time and purges its undelivered events.
- **Server membership isolation:** A device only fetches events routed through servers it is a member of and currently has joined on this device.
- **No retroactive history:** A newly registered device receives only messages sent after its registration. Once acknowledged and delivered, messages are deleted from the server and live only on user devices.

## Friends, message requests, spam and blocks

### Friends

- Friends are established through mutual consent: `SendFriendRequest` and `AnswerFriendRequest(accept = true)`.
- Each party provides the other with a 64-byte cryptographic permission slip (`sign_slip`).
- The Friends page provides tabs for **Online**, **All**, **Pending**, **Message requests**, and **Blocked**, with unread and pending count badges.

### Message requests

- When a member who is not your friend messages you, the conversation appears under **Message requests** in the home sidebar and on the Friends page.
- Message requests produce no desktop notifications or sounds.
- A sticky `RequestBar` at the bottom of the chat lets you **Accept**, **Ignore**, or **Block**.

### Spam threshold

- If you decline friend requests from the same user 3 times (`SPAM_AFTER_DECLINES`, 3 declines), subsequent requests from that user are classified as spam.
- Spam requests are placed in a collapsed "Spam" section on the Pending tab, making no sounds and showing no badges.

### Blocking

- Blocking a user immediately clears friendship and pending requests locally.
- Incoming messages and friend requests from blocked users are rejected with `UNDELIVERABLE`.
- Typing indicators sent to blocked users silently succeed on the server without delivery.
- The user's block list is synced to connected servers via `UpdatePrivacy` (supporting up to 10,000 blocked IDs, `MAX_BLOCKED_IDS`).

### Privacy settings

Users control incoming requests in **User Settings → Privacy**:
- **Friend requests:** Toggle whether server members can send friend requests (`allow_friend_requests`).
- **Message requests:** Toggle whether non-friends can send direct messages (`allow_message_requests`). When disabled, strangers receive `MESSAGE_REQUESTS_CLOSED` unless they hold a valid permission slip.

## Local database and sealing at rest

Each identity stores private conversations and cryptographic keys in a local SQLite database at `<app data>/private/<public_key_hex>.db`.

### Key management and sealing

- **Master key:** A 32-byte cryptographically secure random key generated via `Random.secure()` and stored in the system keychain via `FlutterSecureStorage` under `opencord.private.key` (or profile-isolated `opencord.<profile>.private.key`).
- **Key derivation:** Two 32-byte keys are derived via HMAC-SHA256 from the master key:
  - `pickle` key (label `b"opencord-private-pickle-v1"`): encrypts vodozemac account and Olm session states.
  - `seal` key (label `b"opencord-private-seal-v1"`): encrypts message content, contact display names, and pending outbox payloads using XChaCha20-Poly1305 with a 24-byte random nonce and authenticated column/row labels.
- **Plaintext metadata:** Timestamps, message IDs, and public keys remain unsealed to permit indexed queries and message ordering.

### Fresh start recovery

If the database file is corrupted or the system keychain key cannot open it:
1. The corrupted file is renamed to `<hex>.db.unreadable-<timestamp>`.
2. A new database is initialized with a fresh device registration.
3. The app informs the user with a dialog: *"Your private messages on this device couldn't be opened, so this device starts fresh. New messages will arrive normally."*

---

## Checking it by hand

You can test private messages end-to-end on a single development machine using two isolated profiles and a local server.

### Setup

1. Start a local server:
   ```bash
   cargo run -p opencord-server -- --data-dir target/test-server
   ```
2. Open terminal 1 for Alice:
   ```bash
   OPENCORD_PROFILE=alice flutter run -d linux
   ```
3. Open terminal 2 for Bob:
   ```bash
   OPENCORD_PROFILE=bob flutter run -d linux
   ```
4. Join both Alice and Bob to the local server using the invite link printed by the server.

### Scenario 1: Friend request with one side offline

1. Close Bob's app.
2. In Alice's app, open the member list, click Bob's profile, and click **Add friend**.
3. Alice sees the button change to **Request sent**.
4. Re-open Bob's app (`OPENCORD_PROFILE=bob flutter run -d linux`).
5. Bob sees a badge on **Friends → Pending**. Bob clicks **Accept**.
6. Both Alice and Bob now see each other in their **Online** / **All** friends list.

### Scenario 2: Messages both ways with one side offline

1. Close Bob's app.
2. In Alice's app, open Bob from the Friends list or Home sidebar and send: "Hey Bob, offline test!".
3. The message shows sent status with a checkmark.
4. Launch Bob's app.
5. Bob receives the message immediately upon catch-up, unread count increments, and desktop notification shows.
6. Close Alice's app.
7. In Bob's app, reply: "Got it while offline!".
8. Re-open Alice's app.
9. Alice sees Bob's reply cleanly decrypted and placed in order.

### Scenario 3: A second device of one identity

1. Start a second instance for Alice:
   ```bash
   OPENCORD_PROFILE=alice-laptop flutter run -d linux
   ```
2. In `alice-laptop`, restore or import Alice's identity secret.
3. Send a message from Bob to Alice.
4. Both Alice's main instance and `alice-laptop` receive and decrypt the message.
5. Send a reply from `alice-laptop` to Bob.
6. The outgoing message appears on both `alice-laptop` and Alice's main instance as a sent copy, and on Bob's app.

### Scenario 4: Moving a chat and Move too

1. Have Alice and Bob join a second shared server (e.g. Server 2).
2. In Alice's chat with Bob, click the relay chip in the header (showing Server 1).
3. Select Server 2 from the dropdown.
4. Alice's chat shows a wavy `RelayDivider` with Server 2's name.
5. Bob's chat shows a `MoveCard` notifying that Alice moved the chat to Server 2, with a **Move too** button.
6. Bob clicks **Move too**. Bob's outgoing relay updates to Server 2, inserting a wavy divider.

### Scenario 5: Message requests on and off

1. Using fresh profiles for Charlie and Dana who are not friends:
2. Charlie sends a direct message to Dana.
3. Dana sees the message appear in **Message requests** with no notification sound or unread badge on the Home rail icon.
4. Dana opens the chat and sees the `RequestBar` with **Accept**, **Ignore**, and **Block**.
5. In Dana's app, go to **User Settings → Privacy** and turn off **Message requests**. Click **Save changes**.
6. From a third non-friend account (Eve), attempt to message Dana.
7. Eve sees an error dialog: *"Dana only accepts messages from friends."*

### Scenario 6: Blocking

1. In Alice's app, open Bob's profile or right-click Bob in the sidebar, and select **Block**.
2. Confirm the block dialog.
3. Bob disappears from Alice's friends list and appears under the **Blocked** tab.
4. In Bob's app, attempt to message Alice.
5. The message fails with an undeliverable notice: Bob's message cannot be delivered.
6. In Bob's app, typing in the chat produces no typing indicator on Alice's device.
7. Alice unblocks Bob from the Blocked tab. Bob can message Alice again (as a message request).

### Scenario 7: Restarting both apps mid-conversation

1. While Alice is actively typing in the composer with unsent text, close both Alice's and Bob's apps abruptly (`SIGKILL` or closing windows).
2. Restart Alice's app. Alice's draft text is preserved in the composer.
3. Send the message.
4. Restart Bob's app.
5. Bob connects, catches up, decrypts the message, and marks it read. The conversation history remains intact on both sides without duplicate or lost messages.

### Scenario 8: A server that relays only between friends

1. As the server's owner, open **Server settings → Private messages** and turn on **Only between friends**. Save.
2. Bob, who isn't Alice's friend, messages Alice through this server. He's told the server only carries private messages between friends, and the server isn't offered in his relay menu for Alice.
3. Bob sends Alice a friend request; it goes through. Alice accepts.
4. Bob messages Alice again, at once: it's delivered.
5. Alice removes Bob as a friend. Bob's next message is refused again.
6. Turn **Private messages** off. Both apps stop offering the server for private messages; turn it back on and they register again.
