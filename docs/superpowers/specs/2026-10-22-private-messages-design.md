# Private messages, friends and offline delivery: design

Date: 2026-10-22. Status: written from the owner's request and two answers.
Waiting for the owner's approval. The plan is written from this document and
handed to a cheaper implementer model (Gemini), which hands the riskiest
tasks to Claude. The plan, not this file, carries file-by-file instructions.

## 1. Intent

Opencord has no way to talk to one person privately. The profile's
"Message" button is a placeholder ("Direct messages are coming in a later
phase"), and there are no friends. This project adds:

- **Friends.** A profile gets an "Add friend" button. Requests travel through
  the server you both share, wait there while the other person is offline,
  and the answer comes back the same way.
- **Private messages, end-to-end encrypted.** Every message gets its own key,
  as in Signal and WhatsApp. Servers only pass messages along. They keep a
  message only while it waits for an offline device, and delete it once that
  device confirms it has it. History lives only on people's devices.
- **A home for it all.** The Settings button at the top of the server rail
  becomes a **Private messages** button, as Discord's home button. It opens a
  list of conversations on the left. With no conversation open, the main
  area shows friends (online and offline), friend requests, message
  requests and blocked people.
- **Message requests.** Messages from someone who isn't your friend wait in
  "Message requests", without notifications, until you accept them.
- **Privacy settings.** Anyone can turn off friend requests and message
  requests. Every server they're on learns the setting and refuses those
  events. The sender sees an error dialog that says why.
- **A chosen relay server.** Each conversation goes through one server you
  both share. Either person can move it to another shared server. A
  patterned divider with the server's name marks where each move happened.

Why: chat without private conversations isn't a Discord replacement. And
people who run their own servers want private messages that no server, not
even their own, can read.

## 2. Goals, non-goals, anti-goals

**Goals**

- Works on **Linux, Windows and macOS** desktops, like the rest of the app.
- **Every device of an identity** gets private messages (owner's choice).
  Each device has its own keys. A message is encrypted once for each of the
  other person's devices, and once for each of the sender's own other
  devices, so all of them show the conversation. A device has the messages
  sent after it was set up, not older ones.
- Nothing a server stores can be read by the server: messages, edits and
  relay moves are encrypted. Friend requests, answers and typing are
  routing metadata and travel in the clear (see §10.9).
- A message or friend event for an offline device waits on the server until
  that device fetches it and confirms. Nothing is lost when a connection
  drops mid-delivery or the app is killed mid-processing.
- The server enforces privacy settings and blocks. The client applies the
  same rules again for events that slip through.

**Non-goals (this plan)**

- Files, images and voice messages in private messages (owner's choice:
  later). The composer has no attach or mic button in a private chat.
- Group conversations. Every conversation has exactly two people.
- Calls in private conversations.
- Copying history to a new device, or backing it up.
- Replies, reactions, pins and search in private chats.
- Verifying a device on its own. Devices are trusted when the person's
  identity key signed them (§10.2); comparing the identity key's
  fingerprint covers all of a person's devices.
- A real "Leave server" request. Removing a server in the app still only
  forgets it on this device (§9.4 copes with that).
- The mobile layout.

**Anti-goals (a failure even if everything "works")**

- A server, or anyone with its database, can read a private message.
- A server can make your client accept a device, or a message, as someone's
  when that person never signed it.
- A message is lost because a connection dropped or the app was killed
  between receiving and saving it.
- A message shows twice, or a duplicate delivery shows as "couldn't be
  decrypted".
- Message requests or friend requests from people you declined three times
  (spam) make a sound or a badge.
- Someone you blocked can tell they're blocked from the errors they get.
- The private messages database on disk can be read without the key in the
  system keyring.

## 3. Constraints and existing pieces to build on

- **Identity.** A person is their Ed25519 key, the same on every server
  (Phase 1 plan §1). Each server gives them its own numeric user id. The
  member list (`Member.user.public_key`) maps one to the other, so the
  client can tell the same person apart on every server they share.
- **Servers are independent.** One `opencord-server` is one community; the
  client connects to many at once. There is no central service, so private
  messages must travel through a server both people belong to.
- **Server-side membership outlives the app's "Leave server".** Removing a
  server only forgets it locally; the person stays a member until kicked or
  banned. So "is a member" doesn't mean "will connect again".
- **Existing routing:** `AppState::broadcast` with `Audience::User(id)`
  reaches all of a user's sessions; `SessionRegistry::for_user` and
  `has_user` exist. Events carry a per-session `seq` and replay on resume
  (60 s, 1,000 events). Frames over 1 MiB are refused.
- **Client core:** one connection task per server (`connection.rs`), with
  `Client::request(server_key, kind)` for requests. The core keeps no
  database today, only `opencord.json`. The media cache's key lives in the
  system keychain (`MediaCacheKeyStore`); the private messages key will
  sit beside it.
- **App:** the rail's top button (`server_rail.dart`, `OcIcons.menu`,
  "Settings") opens user settings. The user panel at the bottom of the
  channel sidebar already has a settings cog (`user_panel.dart`), and
  Ctrl+, and the macOS menu open settings too, so nothing is lost when the
  rail button changes job. The chat widgets (`MessageList`, `Composer`)
  are tied to a channel (`ChannelRef`). The rows and bubbles under them
  (`MessageItem`, `MessageLine`, `MessageBubble`, `CompactMessage`,
  `MarkdownView`) can be reused.
- **Settings wait for the save bar** (`GEMINI.md`): the new privacy page
  does too.
- **Licences:** server AGPL-3.0-or-later; `opencord-common`,
  `opencord-core`, `proto/` and `app/` MPL-2.0. New dependencies must be
  MIT, Apache-2.0, BSD, ISC or Zlib.
  - **vodozemac 0.11.1** (Apache-2.0) and everything it pulls in pass:
    checked with `cargo tree --target all`. It uses the workspace's own
    `ed25519-dalek 3`, `sha2 0.11`, `chacha20poly1305 0.11` and
    `prost 0.14`.
  - Signal's libsignal is AGPL and not on crates.io, so it's out.

## 4. The owner's decisions

From the request:
- The rail's top-left button opens private messages, as Discord's home
  button does. Conversations are listed on the left. With none open, the
  main area shows friends (online and offline), friend requests, message
  requests and blocked people.
- "Add friend" is on the profile. A friend request goes through the server
  where it was sent. An online target gets it at once; an offline target
  gets it when they next connect. Accept and decline travel back the same
  way, waiting if the sender is offline.
- Declining someone's friend request three times moves their later requests
  into a collapsed **Spam** section.
- Message requests are messages from people who aren't your friends. They
  wait in their own list, without notifications, as in Discord.
- The Blocked list shows each blocked person's current name if you share a
  server, else the name they had when last seen.
- Friend requests and message requests can each be turned off in privacy
  settings, and that setting always syncs to every server the person is on.
  The client drops such events, the server refuses to route them when it
  knows the setting, and the sender sees an error dialog.
- Messages are end-to-end encrypted with a key per message, as Signal and
  WhatsApp do. The server is a middleman. It saves nothing except undelivered
  messages, and only until the recipient comes online and gets them. Messages
  are stored in the client's database.
- A conversation shows which server relays it. Either person can switch to
  another server they both share. The other person gets a message with a
  button to switch too, and only sees it if they're on that server. A
  patterned divider with the server's name marks the change.

From the two questions:
- **Every device** that uses the identity gets private messages.
- **Text now, files later.** Text with emoji, edit and delete.

## 5. Words used here

- **Relay server** (in the UI: "chatting through *Server*"): the server a
  conversation's messages travel through. Both people must be members.
- **Device:** one install of the app with an identity loaded. It has its own
  encryption keys and a random 16-byte **device id**.
- **Pending event:** what the request calls an "unread event". Something
  waiting on a server for one device: a private message, or a friend
  request, answer, cancellation or removal. It's deleted when that device
  acknowledges it, or when it expires.
- **Permission slip:** a signature by person B saying "A may message me".
  Any server can check it without remembering anything (§10.7).
- **Contact:** anyone the client has a relationship with: a friend, a
  pending request, a conversation, or a block. Contacts are kept by public
  key, so they're the same person on every server.

## 6. What people see

### 6.1 The Private messages button and the home view

- The rail's top button becomes **Private messages**: a chat-bubble icon,
  tooltip "Private messages". It's selected (the same states as a server
  icon) while the home view is open. Its badge counts unread messages in
  conversations that aren't muted, plus incoming friend requests that aren't
  spam. Message requests never add to it.
- Selecting it opens the **home view**. The channel sidebar is replaced by
  the **home sidebar**:
  - header (60 px, like the server header): "Private messages";
  - **Friends** row (people icon), with the count of incoming non-spam
    requests;
  - **Message requests** row, with its count (no badge elsewhere);
  - a section label "DIRECT MESSAGES" and the conversations, the most
    recent first. Each row shows the avatar with presence, the name and an
    unread badge. A ✕ on hover hides the conversation until its next
    message;
  - the existing user panel at the bottom (mute, deafen, settings cog).
- The member panel is hidden in the home view. The right side of a
  conversation's header has no members toggle.
- The home view is remembered like the open server (`navigationProvider`),
  so the app reopens where it was. With no servers at all, the home view
  shows the existing "No servers yet" content in its main area until a
  friend or conversation exists.
- A conversation opened from the home sidebar fills the main area (§6.3).
  With none open, the main area is the **Friends page** (§6.2).

### 6.2 The Friends page

A 60 px header with "Friends" and tabs: **Online · All · Pending · Message
requests · Blocked**. Each tab is a list with a count in its section label
("ONLINE — 3").

- **Online / All:** friends, by name. Each row has the avatar with presence,
  the name, a presence line, a **Message** icon button and a ⋯ menu
  (**Remove friend**, **Block**). Clicking the row opens the conversation.
- **Pending:**
  - incoming requests, each with **Accept** (✓) and **Decline** (✗) buttons;
  - outgoing requests, each with **Cancel** (✗);
  - a collapsed **Spam** section (with its count) at the bottom, holding
    incoming requests from people whose requests you've declined three or
    more times. It has the same buttons, and nothing in it notifies.
- **Message requests:** one row per person, with their name and the first
  line of their latest message. Clicking it opens the conversation in
  request mode (§6.3).
- **Blocked:** each blocked person, named as §7.5 says, with **Unblock**.
- Empty tabs say so plainly ("No one's online right now.", "No pending
  requests.", "No message requests.", "You haven't blocked anyone.").

### 6.3 A conversation

- **Header:** avatar with presence, name, presence label; a lock icon
  (tooltip "End-to-end encrypted. Only you and *name* can read these
  messages."; click shows their identity key fingerprint with Copy); and a
  **relay chip**: the server's icon and "via *Server*" with a ▾. The chip
  opens a menu of the servers you share where the other person is a member
  and this device is connected (the current one checked). Picking another
  moves the chat (§8).
- **Messages:** the same rows and bubbles as a channel chat, in the user's
  chosen message style. Your own messages can be edited and deleted; any
  message can be copied. There are no replies, reactions, pins, mentions
  menu or attachments. A message that failed to send shows the channel
  chat's failed state with **Retry** and **Delete**.
- **Dividers:** a wavy line on each side of the server's icon and name,
  like Claude Code's compaction separator (the owner's choice on
  2026-10-24, replacing diagonal hatching). The name opens that server when
  this device has it. One sits at the top of every conversation (where it
  started) and one at each move (§8).
- **Move cards:** when the other person moved the chat, a card says
  "*Name* moved this chat to *Server*", with **Move too** when this device
  can use that server (§8).
- **System lines:** a muted, centered line for "This message couldn't be
  decrypted" and the like.
- **Composer:** text only. It's the channel composer's look and keys (Enter
  sends, Shift+Enter adds a line, ↑ edits your last message, Esc cancels
  editing), with no attach, mic, commands or emoji-from-server. Typing
  sends a typing signal (§9.3). "*Name* is typing…" shows above it, as in
  channels.
- **Disabled composer**, with the reason in its place, when:
  - you blocked them: "You blocked *name*." with **Unblock**;
  - the relay can't be used and no other shared server is connected: "You
    don't share a connected server with *name* right now.";
  - you share no server at all: "You no longer share a server with
    *name*.".
- **Request mode** (a message request you haven't accepted): the composer is
  replaced by a bar: "*Name* wants to message you." with **Accept**,
  **Ignore** and **Block**. Ignore deletes the conversation from this device
  and tells no one.

### 6.4 Profiles and member menus

- **Profile dialog** (`member_profile.dart`): the placeholder Message button
  becomes real. Next to it is a friend button that shows the state:
  **Add friend**, **Request sent** (click to cancel), **Accept request**,
  or **Friends** (a check, inert). A ⋯ menu has **Block** or **Unblock**.
  None of these appear on your own profile.
- **Message** opens the conversation (creating it if needed). A new
  conversation's relay is the server the profile was opened on. An existing
  conversation keeps its relay.
- **Member right-click menu** (`member_actions.dart`): **Message**, the same
  friend action, and **Block** / **Unblock**.

### 6.5 Privacy settings

A new **Privacy** page in user settings, using the save bar:
- **Friend requests**: "Let people send you friend requests." On by default.
- **Message requests**: "Let people who aren't your friends message you."
  On by default.

The settings are kept in the private messages database and sent to every
connected server on Save, and again each time a server connects (§9.3).

### 6.6 Notifications and badges

- A new message in a conversation that's open (not a request), not muted
  and not from someone blocked: a desktop notification through the existing
  notification policy (focus, do not disturb, preview setting) and the
  unread badge. Its title is the person's name.
- An incoming friend request that isn't spam: a notification "*Name* sent
  you a friend request", and the Friends row and rail badges count it.
- Message requests, spam requests, answers, cancellations and removals:
  no notification.
- Conversations can be muted from their row's menu (**Mute** / **Unmute**).
  Muted ones get no notification and no badge, only the bold row.
- The rail button's count joins the window title's unread count
  ("(3) Opencord") and the tray, as server mentions do.

### 6.7 Errors people see

Each failure the sender causes opens a small dialog (`OcDialog`) with a
title and the reason. Texts (the server's own text is for logs only):

| Code | Dialog |
|---|---|
| `FRIEND_REQUESTS_CLOSED` | "Couldn't send friend request" / "*Name* isn't accepting friend requests." |
| `MESSAGE_REQUESTS_CLOSED` | "Couldn't send message" / "*Name* only accepts messages from friends." |
| `UNDELIVERABLE` | "Couldn't send message" / "Your message couldn't be delivered." For a friend request: "Couldn't send friend request" / "Your friend request couldn't be sent." |
| `PRIVATE_MESSAGES_UNAVAILABLE` | "Couldn't send message" / "*Name* can't get private messages through *Server*. Try another server you share." |
| `CONFLICT` on a friend request | "*Name* already sent you a friend request. Accept it in Friends → Pending." or "You already sent *Name* a friend request." |
| `RATE_LIMITED` | the app's existing "slow down" handling |

A message refused this way stays in the chat as failed, so it can be retried
later or deleted. `UNDELIVERABLE` covers being blocked and is worded so it
doesn't reveal a block.

## 7. Relationships

All of this is kept per contact on each device, in its private messages
database (§11).

### 7.1 Friend requests

1. A opens B's profile on server X and presses **Add friend**. A's client
   sends a friend request to X, with A's permission slip for B (§10.7).
2. X checks the rules (§9.3). It records the request and leaves a pending
   event for each of B's devices on X, and one for each of A's other
   devices, so they show it as sent. Online devices get it at once.
3. B's devices show it in Pending (or Spam), each acknowledging it.
4. B accepts or declines on one device. The answer goes to X, which deletes
   the request. It leaves a pending event for each of A's devices and each
   of B's other devices. An accept carries B's permission slip for A.
5. Accepted: both become friends on every device that gets the answer.
   Declined: the request disappears from A's Pending, quietly. B's device
   adds one to its decline count for A.
6. A can cancel while it's pending. X deletes the request, removes any
   undelivered copies of it, and tells B's devices and A's other devices.

Rules:
- Only one pending request per pair on a server. A request to someone who
  already sent you one is refused with `CONFLICT`. The app turns the button
  into **Accept request** anyway, since it should already know.
- **Crossed requests** on two different servers: a device that receives a
  request from someone it has an outgoing request to accepts theirs and
  cancels its own.
- A request from someone who's already a friend is answered "accepted"
  automatically by the receiving device.
- Answering or cancelling goes through the server the request belongs to.
  If this device isn't connected to it, the dialog says "Couldn't answer:
  *Server* isn't connected right now." and nothing changes.
- A request arriving when this device's Friend requests setting is off, or
  from someone blocked, is acknowledged and dropped.
- After **three** declines of the same person's requests (counted on each
  device), their later requests land in Spam.

### 7.2 Message requests

- A message from someone who is neither a friend nor in an accepted
  conversation, and not blocked, opens a conversation in **request** state.
  It shows only under Message requests, with no notification.
- **Accept** makes it a normal conversation and sends them an encrypted
  "request accepted" note carrying this person's permission slip (§10.7).
- **Ignore** deletes it from this device. If they write again, a new request
  appears.
- **Block** blocks them (§7.4).
- When this device's Message requests setting is off, such messages are
  acknowledged and dropped.
- A conversation is accepted on a person's side when they're friends, or
  they sent the other a message or a friend request, or they accepted the
  other's request. A conversation with a friend is always accepted.

### 7.3 Removing a friend

**Remove friend** asks for confirmation, then sends a removal through
**every connected server you share**, so it reaches as many of the other
person's devices and your other devices as possible. A device that gets it
twice ignores the second, since they're no longer friends. Both sides stop
being friends. The conversation stays as it is.

### 7.4 Blocking

**Block** asks for confirmation. Then, on this device:
- the contact is marked blocked;
- if you're friends, a removal is sent (§7.3);
- an incoming request is declined and an outgoing one cancelled;
- the conversation disappears from the lists, and the person appears under
  Blocked;
- every connected server where they're a member gets the new block list
  with the privacy settings (§9.3).

Messages and requests from a blocked person are dropped by the client too,
in case a server hasn't caught up. **Unblock** reverses the mark and syncs
again. A conversation with history reappears; friendship doesn't come back.

You can block someone you've never talked to, from their profile.

### 7.5 Names, avatars and presence

- **Name:** their display name on the relay server if they're a member
  there; else on the first shared server in rail order; else the name last
  seen. Each device updates "last seen" whenever it sees the person in a
  member list or event. Rail order is the app's, so the app resolves names,
  avatars and presence. The core gives it the shared servers and the last
  seen name.
- **Avatar:** from the same server as the name, through the existing
  `MemberAvatar`. Without a shared server, initials.
- **Presence:** from the relay server if they're a member there and it's
  connected, else from the first shared, connected server that shows them
  as not offline; else offline.

## 8. The relay server and moving a chat

- Each conversation has a relay server on each side, kept by the server's
  id from `Hello` (`SavedServer.server_id`), not its address.
- **Start:** a conversation started from a profile uses that server. A
  message request uses the server it arrived through. A conversation started
  from the Friends page uses its last relay; failing that, the server where
  the friendship began; failing that, the first shared server in rail order.
  The app makes that last choice and passes the server to the core.
- **Moving:** A picks server Y from the relay chip. Y must be a server where
  B is a member and A's device is connected.
  1. A's client sends a "moved to Y" note (encrypted, like a message)
     **through Y**, to B's devices and A's other devices.
  2. If Y refuses (B isn't a member there or has no device registered there,
     or Y has private messages turned off), nothing changes, and A sees
     "Couldn't move the chat: *name* can't get private messages through
     *Y*."
  3. If Y accepts, A's relay becomes Y, and a divider "Y" goes into A's
     chat at that point. A's other devices do the same when the note
     arrives.
  4. B's device shows the move card. **Move too** appears only when this
     device is connected to Y and A is a member of Y. Otherwise the card
     says "*Name* moved this chat to *Y*, a server this device isn't on." On
     **Move too**, B's relay becomes Y and B's divider goes in.
- Until B moves, A writes through Y and B through X. Both still get
  everything, since each device listens on all its servers. Only the
  dividers mark who moved when.
- If the relay can't be used (the server isn't saved or connected on this
  device, or the person stopped being a member there), the chip shows a
  warning and the composer says why (§6.3). The person picks another server
  from the chip.
- A device only gets messages that travel through servers it's connected
  to. When a conversation's relay is a server this device hasn't added, the
  header says "This chat goes through *Server*, which this device hasn't
  joined." This is a limit of multi-device without a central server.

## 9. Server

### 9.1 What it stores

All in its SQLite database, migration `0010_private_messages.sql`:

| Table | What | Kept until |
|---|---|---|
| `private_privacy` | per user: allow friend requests, allow message requests | the user leaves |
| `private_blocks` | (owner, blocked) pairs | replaced by the owner's next sync; the owner leaves |
| `private_devices` | per user and device id: Curve25519 key, Ed25519 key, the identity's signature over them, the fallback key and its signature, `created_at`, `last_seen_at` | unregistered, evicted, expired, or the user leaves |
| `private_one_time_keys` | per device: unclaimed one-time keys and signatures | claimed, or the device goes |
| `private_friend_requests` | id, from, to, `created_at`; one per pair | answered, cancelled, or either person leaves |
| `private_pending` | id (Snowflake), user, device, kind, the encoded pending event, an optional friend request id, `created_at` | acknowledged, expired, pushed out by the cap, or the device goes |
| `private_recent_sends` | (sending device, send id), `created_at` | 24 h |

Pending events and one-time keys belong to their device by foreign key with
`ON DELETE CASCADE`. Privacy settings and blocks are also kept in memory,
loaded at startup, because every request checks them.

### 9.2 Pending events: store, deliver, acknowledge, expire

- **Always stored first.** Every routed event (message, friend request,
  answer, cancellation, removal) is written as one row per target device,
  then pushed to that device's sessions if it has any. It stays until that
  device acknowledges it. So a device that drops mid-push gets it again.
- **Session bound to a device.** After `Ready`, the client registers its
  device on that session. From then on, events for that device reach that
  session.
- **Catching up is pulled, not pushed.** After registering, the client asks
  for its pending events in pages (`FetchPendingEvents`, after an id, up to
  100 events and 768 KiB a page), processes and acknowledges each page, and
  asks again until none are left. Live events pushed meanwhile may repeat
  one from a page; the client skips repeats by id.
  - Why: a device can have thousands waiting. Pushing them all at once would
    overflow the session's queue and close the connection.
- **Acknowledging** deletes rows by id, for the device this session holds.
  It also refreshes the device's `last_seen_at`.
- **Expiry**, every hour:
  - pending events older than `undelivered_days` (default 30);
  - devices not seen for `device_expiry_days` (default 60), with their keys
    and pending events;
  - recent-send ids older than 24 h.
- **Cap:** at most `max_pending_per_device` (default 5,000) rows per device.
  A new row over the cap deletes that device's oldest. Only a device away
  for a long time loses anything, and it's the oldest it loses.

### 9.3 Routing rules, checked cheapest first

**Register a device** (once per session, after `Ready`):
1. The feature is on.
2. The device's signature verifies against this user's identity key, and the
   keys are well formed.
3. Each one-time key's and the fallback key's signature verifies against the
   device's Ed25519 key.
4. Upsert the device (same id with different keys: replace them and drop its
   old one-time keys), add the keys, set the fallback key and
   `last_seen_at`. A user with more than **10** devices on this server loses
   the one seen longest ago.
5. Bind the session. Answer with the number of unclaimed one-time keys and
   of pending events.

A session that registers a second, different device is refused. A device
is bound to one session per server at a time: a new binding (a reconnect
whose old session hasn't expired yet) moves it, and the old session stops
getting that device's events.

**Privacy and blocks** (`UpdatePrivacy`, sent on every `Ready` after
registering, and on every change): replaces the user's settings and block
list on this server. Block ids that aren't members are ignored. The client
also sends it again when someone on its block list joins (`MemberJoin`).

**Send a friend request:**
1. Rate limit: 10 per 10 minutes per sender. At most 100 outgoing pending
   requests per sender on a server.
2. The target is a member and not the caller.
3. The target blocked the caller: `UNDELIVERABLE`.
4. The target turned off friend requests: `FRIEND_REQUESTS_CLOSED`.
5. The target has no device registered here: `PRIVATE_MESSAGES_UNAVAILABLE`.
6. The slip is valid (§10.7), else `INVALID_ARGUMENT`.
7. A request between the pair exists, either way: `CONFLICT`.
8. Write the request and the pending events (target's devices; caller's
   other devices). Answer with the request id.

**Answer** (accept or decline): the request exists and the caller is its
target (else `NOT_FOUND`); an accept's slip is valid; delete the request,
checking that it still existed; pending events to the requester's devices
and the caller's other devices.

**Cancel:** the caller sent it. Delete it and its undelivered
`FriendRequestReceived` rows. Pending events to the target's devices and
the caller's other devices.

**Remove friend:** the target is a member. Pending events to the target's
devices and the caller's other devices. The server keeps no friend list;
the receiving client ignores a removal from someone who isn't a friend.

**Send a private message:**
1. The session has a registered device: else
   `PRIVATE_MESSAGES_UNAVAILABLE`.
2. Rate limit: 5 per 5 s per sender and recipient.
3. The recipient is a member and not the caller.
4. The recipient blocked the caller: `UNDELIVERABLE`.
5. The recipient turned off message requests: the slip must be valid, else
   `MESSAGE_REQUESTS_CLOSED`. (With requests on, a missing slip is fine.)
6. Shape: at most 20 encrypted copies, each at most 32 KiB; the send id is
   16 bytes.
7. A send id this device already sent in the last 24 h: answer success
   again and store nothing (the client is retrying a lost answer).
8. The copies' (user, device) targets must equal exactly the recipient's
   devices plus the caller's other devices registered here. Otherwise
   `DEVICE_LIST_CHANGED`. The recipient must have at least one device:
   else `PRIVATE_MESSAGES_UNAVAILABLE`.
9. In one transaction: the pending rows (applying the cap) and the
   recent-send id. Then push them, and answer with the server's time,
   which both sides use as the message's time.

**Typing** (`StartPrivateTyping`): the same checks as a message through
step 5. A failing check is answered with success and nothing sent, so
typing never reveals a block. It goes to the recipient's sessions on this
server, unstored.

**Fetch devices** (for the caller and one other member) and **claim keys**
(up to 20 devices per call: one unclaimed one-time key each, taken
atomically, or the fallback key when none are left) need a registered
device. They are rate limited (30 claims per minute).

### 9.4 Cleanup

- When a member is kicked or banned, delete their devices (so their keys and
  pending events go), privacy settings, blocks, and every friend request to
  or from them with its undelivered pending rows.
- When the app removes a server ("Leave server" in the rail), the core first
  unregisters its device there (best effort; if the server is unreachable,
  expiry removes it later). Then senders get
  `PRIVATE_MESSAGES_UNAVAILABLE` there instead of messages piling up for a
  device that will never come back.

### 9.5 Limits and config

New `[private_messages]` section in `opencord.toml`, each with an
`OPENCORD_PRIVATE_*` override:

| Setting | Default | Range |
|---|---|---|
| `enabled` | `true` | |
| `undelivered_days` | 30 | 1–365 |
| `max_pending_per_device` | 5,000 | 100–100,000 |
| `device_expiry_days` | 60 | 7–365 |

Fixed limits: 10 devices per user per server; 20 one-time keys kept per
device per server (the client tops up); 20 copies per message, 32 KiB each;
100 events or 768 KiB per pending page; message text 1–4,000 characters
(the channel rule, `validation::message_content`, checked by the clients,
since the server can't see it).

`Ready` gains `private_messages: bool`, true when `enabled`.

## 10. End-to-end encryption

### 10.1 Library

**vodozemac** (Matrix's Rust implementation of Olm, audited): Olm is the
Double Ratchet with a triple Diffie-Hellman start, the same construction
Signal uses. Every message gets its own key, old keys are deleted (forward
secrecy), and a new Diffie-Hellman step heals the session after a
compromise. Use its `Account`, `Session`, `OlmMessage` and encrypted
pickles. Don't write ratchet or key agreement code here.

Facts about 0.11.1 the design relies on:
- An `Account` keeps up to 5,000 private one-time keys, dropping the oldest
  beyond that. `max_number_of_one_time_keys()` says 50 per server.
- A receiving chain keeps 40 skipped message keys and accepts gaps up to
  2,000. Out-of-order delivery within those limits decrypts.
- An already used message index fails with a missing-message-key error:
  that's how duplicates and replays show (§10.5).
- `generate_fallback_key()` keeps the previous fallback key until the next
  generation or `forget_fallback_key()`.

### 10.2 Devices and their keys

- Each device creates one `Account` and a random 16-byte device id when the
  private messages database is first opened for an identity.
- **Device keys:** the account's Curve25519 and Ed25519 keys, signed by the
  person's identity key (Ed25519) over:
  `"opencord-device-v1" || device_id (16) || curve25519 (32) || ed25519 (32)`.
  The server checks this signature at registration. **Every client checks it
  again** against the public key it already knows for that person, and
  ignores a device that fails. That's what stops a server from adding its
  own device to someone and reading along.
- **Prekeys:** one-time keys and the fallback key, each signed by the
  device's Ed25519 key (`Account::sign`) over:
  `"opencord-prekey-v1" || device_id (16) || key (32) || fallback (1 byte: 0 or 1)`.
  Checked by the server on upload and by the client on claim.
- **Per server:** the device registers on every connected server that has
  `private_messages`. It keeps 20 unclaimed one-time keys there, topping up
  after registering and after each prekey message that arrives through that
  server. Each server gets its own keys, never the same key twice.
- **Fallback key:** rotated every 30 days. At registration a device always
  uploads its current one, so only a server it hasn't connected to for more
  than 30 days can hand out one that no longer works (60 days after it was
  made).
- These signing payloads live in `opencord-common` (`private.rs`), shared by
  server and client.

### 10.3 Sessions

- Sessions are kept per **peer device** (by its Curve25519 key). There can be
  several, for instance when both sides start one at the same moment.
- **Sending** uses the session that most recently decrypted a message from
  that device. Failing that, the most recently created one.
- **Receiving:**
  - A normal message tries that device's sessions, newest first.
  - A prekey message first tries the existing session it belongs to (its
    session keys match). Failing that, it creates an inbound session, which
    uses up the one-time key.
- At most 5 sessions per peer device are kept; older ones are dropped.
- A new outbound session needs a claimed key from the relay server (§9.3):
  a one-time key, or the fallback key when none are left.

### 10.4 Sending

1. **Resolve the relay** (§8). It must be connected and have
   `private_messages`.
2. **Devices:** fetch the device lists of the recipient and of oneself on the
   relay, unless fetched less than 10 minutes ago. Keep only devices whose
   signature verifies, and drop this device.
3. **Sessions:** for any device without a session, claim keys and create
   outbound sessions. A claimed key whose signature fails means that device
   is skipped for this send.
4. **Encrypt** the payload (§10.6) once per device.
5. **Save before sending.** In one database transaction, save the updated
   sessions and the outgoing message with its encrypted request (the
   *outbox*). Only then send.
   - Why: a session saved after sending can be lost in a crash. The next
     message would then reuse a message key the other side has already
     used, and it would never decrypt.
6. **Send** (§9.3), with a send id: the message id.
   - **Success:** mark it sent with the server's time, and drop the outbox
     copy.
   - **`DEVICE_LIST_CHANGED`:** fetch the device lists again, encrypt again
     for the new set, save again, and resend, once. A second mismatch fails
     the message.
   - **Connection lost or timed out:** keep the outbox. When the relay
     reconnects, resend the **same** encrypted request. The send id makes a
     repeat harmless.
   - **Refused** (§6.7): the message is failed; show the dialog.
7. **Retry** of a failed message resends its saved request if it has one,
   else encrypts again. Messages still sending when the app starts are
   resent once the relay connects, if under 24 h old; older ones become
   failed.
8. **An outbox belongs to its server.** Device lists are per server, so a
   saved request is only ever resent through the server it was encrypted
   for (kept with it). If the conversation moved meanwhile, or that server
   is gone, a retry encrypts again for the current relay.

Edits, deletes, "request accepted" notes, move notes and session resets go
the same way: each is a payload with its own id.

### 10.5 Receiving

All pending events from all servers go through **one worker**, one at a
time, so that two updates to a session never race.

1. **Repeat?** If (server id, event id) is already recorded, acknowledge it
   and stop.
2. **Friend events:** apply §7. A message event goes to step 3.
3. **Sender device:** look up the sender's device among the verified ones.
   If it's unknown, fetch the sender's device list through the server the
   event came from, and verify it. An unverifiable sender means a "couldn't
   be decrypted" line, then step 7.
4. **Decrypt** (§10.3).
   - A missing-message-key error means a duplicate or a replay: drop it
     silently.
   - Any other failure gives a "This message couldn't be decrypted" line in
     that conversation, and starts a session reset (§10.8).
5. **Check the payload** (§10.6): it decodes, and its recipient key is this
   person (or, for a copy from one's own other device, the conversation's
   other person). The content is valid. An edit or delete names a message
   by the same author.
6. **Apply and save, together.** In one transaction: the updated session,
   the message or change, the contact's changes, and the (server id, event
   id) record. Then tell the app.
7. **Acknowledge** to the server it came from. Acknowledgements are batched
   per server, sent at least every 200 ms while any are waiting.

The worker keeps the (server id, event id) records for 35 days (more than a
server keeps an event).

A crash before step 6 commits leaves the session as it was. The event is
still on the server, so it comes again and decrypts the same way.

### 10.6 Payload

A protobuf message (in `proto/opencord/v1/private.proto`, which the server
never decodes):

- `message_id`: 16 random bytes, made by the author's device;
- `sent_at_ms`: the author's clock, for information only. The server's time
  orders and dates messages;
- `recipient_public_key`: the conversation's other person, so a copy to the
  sender's own devices says which conversation it belongs to;
- `slip`: the author's permission slip for the recipient (§10.7), on every
  message and on "request accepted";
- `body`, one of:
  - **text** (1–4,000 characters);
  - **edit** of the author's own message (`message_id`, new text);
  - **delete** of the author's own message;
  - **moved** to a relay (server id, server name);
  - **request accepted**;
  - **session reset** (empty).

### 10.7 Permission slips

- A slip is an Ed25519 signature by B's identity key over
  `"opencord-dm-allow-v1" || B's public key (32) || A's public key (32)`.
  Holding it means "B lets A message them".
- **Where slips come from:**
  - A sending a friend request includes A's slip for B.
  - B accepting includes B's slip for A.
  - Every message's payload carries its author's slip for the recipient.
  - "Request accepted" carries the accepter's slip.

  So every friendship and every accepted conversation leaves each side
  holding the other's slip.
- A client attaches the slip it holds for the recipient to every message
  and typing signal. The server checks it only when the recipient turned off
  message requests (§9.3). Any server can check it, so moving a chat to a
  new server needs no setup there.
- A slip can't be taken back, except by blocking, which every server checks
  first.

### 10.8 Decryption failures and session reset

When a message from a device can't be decrypted (and it isn't a repeat), the
receiver claims a key for that device and starts a fresh outbound session.
It sends a **session reset** payload through the same server. The sender's
device creates the inbound session from it. That session becomes the
newest one to decrypt something, so the sender uses it from then on. At most
one reset per peer device per hour.

### 10.9 What servers see, and what they can't do

- **They see:** who messages whom and when, message sizes, how many devices
  each person has, friend requests and answers made through them, privacy
  settings, the block lists of their members, typing, and slips (which only
  say that two people are allowed to talk).
- **They can't:** read a message, an edit or a move; forge a message from a
  device; add a device to someone (§10.2); make one person look like another
  (the identity is the key).
- **They can:** drop, delay or reorder messages, refuse to relay, or lie
  about who is a member. Replaying an old message does nothing (§10.5).
  The app says so in `docs/private-messages.md`.

## 11. Client storage

- A SQLite database per identity at
  `<app data>/private/<identity public key in hex>.db`, opened by the core
  through `sqlx` (already in the workspace). It has its own migrations in
  `crates/opencord-core/migrations/` and runtime-checked queries, since the
  server's offline query data only covers the server.
- **Key:** 32 random bytes in the system keychain (`PrivateKeyStore`, like
  `MediaCacheKeyStore`), given to the core when it opens the database. Two
  keys are derived with HMAC-SHA256, labelled "opencord-private-pickle-v1"
  and "opencord-private-seal-v1".
  - The **pickle** key encrypts vodozemac's account and session pickles
    (their own encrypted pickle format).
  - The **seal** key encrypts message text, outbox requests and contact
    names, with XChaCha20-Poly1305, a random 24-byte nonce, and the column
    and row as associated data.
  - Timestamps, public keys and states stay plain, so queries can sort and
    filter. The design accepts that a copied database shows who talked to
    whom and when, but not what was said or what anyone is called.
- **What it holds:** the device (id, account pickle, fallback key age);
  verified peer devices; sessions; contacts (friend state, request id and
  server, the friendship's server, blocked, decline count, conversation
  state, relay server id, the slip they gave, last seen name, last activity,
  last read, muted, hidden); messages (id, conversation, direction, kind,
  sealed text, server time, edited time, state, sealed outbox); received
  event records; and privacy settings.
- **A database it can't open** (the keychain lost its key, or the file is
  damaged) is moved aside as `<name>.unreadable-<time>`, and a new one is
  made with a new device. The app tells the person once: "Your private
  messages on this device couldn't be opened, so this device starts fresh.
  New messages will arrive normally."

## 12. Client structure

**Core (Rust)**, a new `private` module behind one service owned by the
`Client`:
- the database and sealing;
- the device and prekeys per server;
- sessions;
- the send pipeline with the outbox;
- the receive worker;
- contacts and their rules (pure functions, unit tested);
- the relay and shared servers;
- a member index per server (user id ↔ public key, and names), kept by the
  connection tasks from `Ready` and member events.

The service learns when each server's `Ready` arrives. It does the
registration, the privacy sync and the catch-up for that server (§9.2) on
its own, after the database is open.

**Dart API:** functions to open the database with the key; list contacts
and a conversation's messages (paged); send, edit, delete and retry; mark
read; mute and hide; friend request send, answer and cancel; remove friend;
accept and ignore a message request; block and unblock; read and save
privacy settings; open a conversation from (server, user); list a
conversation's shared servers; move the relay; and typing. A separate event
stream carries changes (contact changed or removed, message added, changed
or deleted, typing, notices). The app subscribes first, then loads, so it
never misses a change.

**App (Flutter):**
- models, repository and mock;
- `private_state` providers (contacts, conversations, unread, typing,
  presence, names);
- `features/home/` (sidebar, Friends page, conversation, divider, move
  card, request bar);
- the privacy page;
- changes to the rail, navigation, shell, profile, member menu and
  notifications.

## 13. Protocol additions

New file `proto/opencord/v1/private.proto` holds the models and the payload.
Field numbers are given in the plan. Requests:

- `RegisterDevice`, `UploadKeys`, `UnregisterDevice`;
- `FetchDevices`, `ClaimKeys`;
- `FetchPendingEvents`, `AckPendingEvents`;
- `SendPrivateMessage`, `StartPrivateTyping`;
- `UpdatePrivacy`;
- `SendFriendRequest`, `AnswerFriendRequest`, `CancelFriendRequest`,
  `RemoveFriend`.

Events:
- `PendingEvent`: id, time, and one of `FriendRequestReceived`,
  `FriendRequestSent`, `FriendRequestAnswered`, `FriendRequestCancelled`,
  `FriendRemoved`, `PrivateMessage`;
- `PrivateTypingStart`.

Error codes:
- `FRIEND_REQUESTS_CLOSED`;
- `MESSAGE_REQUESTS_CLOSED`;
- `UNDELIVERABLE`;
- `DEVICE_LIST_CHANGED`;
- `PRIVATE_MESSAGES_UNAVAILABLE`.

`Ready.private_messages`. `docs/protocol.md` documents all of it.

## 14. Compatibility

- **Old servers:** they lack `Ready.private_messages`, so the client never
  registers there and never offers them as a relay.
- **Old clients:** they never register a device. People using one can't get
  friend requests or messages, and senders get
  `PRIVATE_MESSAGES_UNAVAILABLE`.
- No existing message, request or table changes meaning.

## 15. Testing approach

- **Shared:** signing payloads and their checks, including a wrong key, a
  flipped byte, a swapped device id, and a fallback key presented as a
  one-time key.
- **Server** (raw protocol clients):
  - a registration with a bad signature is refused;
  - claims use up one-time keys, then fall back;
  - an offline device gets everything after reconnecting, in order;
  - an event unacknowledged when the connection drops comes again;
  - paging stops at 100 events and at 768 KiB;
  - each privacy setting and a block gives its error;
  - a slip lets a message through when requests are off, and a wrong slip
    doesn't;
  - a mismatched device set gives `DEVICE_LIST_CHANGED`;
  - a repeated send id stores nothing;
  - cancelling removes the undelivered request;
  - the cap drops the oldest;
  - expiry removes old events and devices;
  - a kick cleans up;
  - typing never reveals a block.
- **Core** (two or three real clients against real servers):
  - a message both ways, online and offline;
  - a second device of the sender gets copies, and a device added later gets
    only new messages;
  - a restart keeps sessions and history;
  - a duplicate delivery (resume replay plus a pending page) shows once;
  - a crash between decrypting and saving recovers on the next delivery;
  - the outbox resends the same request after a lost answer;
  - a forged device (bad signature) is never used;
  - a session reset recovers from a broken session;
  - a move through a server where the person isn't a member is refused;
  - request, accept, block and spam rules, as pure-function tests and end to
    end.
- **App** (widget tests on the mock):
  - the rail button and badge, and the home view's lists;
  - each Friends tab and its actions, and the Spam section;
  - blocked names;
  - a conversation with its dividers and move card (with and without
    **Move too**), and request mode;
  - the privacy page with the save bar;
  - the profile buttons in every friend state;
  - each error dialog.

## 16. Milestones

1. **E0, foundations:** proto, shared signing payloads and limits, config,
   migration, the `Ready` flag.
2. **E1, server:** devices and keys; pending events with delivery,
   acknowledgement, paging, expiry and cleanup; privacy and blocks; private
   messages and typing; friend requests.
3. **E2, client storage and keys:** database and sealing; device and
   sessions; contacts and their rules.
4. **E3, client delivery:** the service with the member index, registration
   and catch-up; the send pipeline; the receive worker; friends, requests,
   blocks and privacy; conversations, relays and typing.
5. **E4, app:** the Dart API; models, repository, mock and state; the
   Friends page; the conversation; the rail button and home view; profile,
   menus and the privacy page; notifications and badges.
6. **E5, hardening and docs:** a security pass over the whole diff,
   `docs/private-messages.md`, `docs/protocol.md`, README, and the checks by
   hand.

The plan scores each task. Tasks touching cryptography, delivery guarantees,
concurrency or data loss go to Claude: device and key handling, pending
delivery, message routing, the database, sessions, both pipelines, and the
security pass. The rest go to Gemini.

## 17. Calls made without asking (for the owner to check)

1. **Library:** vodozemac (Olm). libsignal's licence rules it out, and
   writing the ratchet ourselves is the risk to avoid.
2. **Moving a chat takes effect at once for the mover.** The note goes
   through the new server, which doubles as the check that the other person
   is there. The other person moves with **Move too**. Each side's divider
   marks its own move. The first divider is at the top of every
   conversation.
3. **Declines go back to the sender silently.** The request just leaves
   their Pending, as in Discord.
4. **Permission slips** let any server enforce "message requests off"
   without keeping friend lists.
5. **Blocks reach servers** (with the privacy settings) so they can refuse
   the blocked person, but the error doesn't reveal the block. Blocking also
   ends a friendship and declines or cancels open requests.
6. **Undelivered events last 30 days**; a device unseen for 60 days is
   dropped; at most 5,000 waiting per device. The owner can change all three
   in `opencord.toml`, or turn private messages off.
7. **Message requests and spam make no sound or badge.** Friend requests
   notify. Conversations can be muted.
8. **Edit and delete** in private chats; delete removes it for everyone.
   There are no replies, reactions or pins (channels don't really have them
   in the protocol either).
9. **Typing indicators** in private chats, never stored, never revealing a
   block.
10. **The database on disk** encrypts text and names but not who-and-when.
    SQLCipher would mean building OpenSSL on every platform.
11. **"Leave server" unregisters the device there** first, so messages don't
    pile up for nobody.
12. **10 devices per person per server**, the oldest dropped beyond that.
13. **A conversation can be hidden** (✕) until its next message, as in
    Discord. Ignoring a message request deletes it from the device.
14. **Failed messages stay** in the chat with Retry and Delete. Unsent ones
    resend for up to 24 h after a restart.
15. **No "add friend by key"** tab: friends are added from profiles, since a
    request needs a server you share.
16. **Names, avatars and presence** come from the relay server first, then
    other shared servers (§7.5).
17. **Crossed friend requests** (each sent the other one, on different
    servers) become a friendship automatically.
18. **A removal goes through every connected shared server**, so most of
    the other person's devices hear about it.
19. **Unread private messages count in the window title and tray**, like
    server mentions.

## 18. Left to the builder

- Exact widget composition, as long as it reuses the chat rows and bubbles
  and follows the existing design system (monochrome, `OcText`, `OcIcons`,
  the app's widgets).
- The wave's amplitude and wavelength.
- Internal module boundaries in the core beyond §12, and the database
  column names.
- How the Dart side caches lists, as long as it reloads after subscribing.
