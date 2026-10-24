# Uploaded songs, played by the server itself: design

Date: 2026-10-24. Status: approved by the owner, then built (D67).

**As built**, where it differs from what follows:

- Uploaded songs use the track source `upload` (shown as Local), not
  `local`, which Lavalink already uses for files on its own machine.
- Covers inside files aren't kept or shown yet.
- Ogg files with Opus inside are refused (symphonia has no Opus decoder).
- "Remove from server" is on one's own songs in search results; queue
  items offer it to administrators only. Recently played has no menu.
- The settings list is called "Uploaded songs".

## 1. Intent

Members want to play songs they have on disk, in a voice channel, on any
Opencord server, including one with no Lavalink at all. Today every sound
the music player makes comes from a Lavalink node, which fetches it from
YouTube, SoundCloud and the like; a song that is only on someone's
computer cannot be played, and a server without Lavalink has no music.

So: a member drags audio files onto the music player. The app uploads
them to the Opencord server, with progress and a cancel button, and they
join the channel's queue. The Opencord server plays them itself: it
decodes the file and sends the audio into the voice channel the same way
Lavalink's audio arrives today. A song the server already has is never
uploaded twice: the app checks the file's hash first, and a known song
joins the queue at once. Uploaded songs stay on the server as its
**local songs**, which anyone can find again in the player's search.

## 2. Goals, non-goals, anti-goals

Goals

- Drop one or more audio files onto the music player to queue them.
- The app hashes each file and asks the server whether it has it. A known
  file is queued at once, with nothing uploaded.
- Unknown files upload with a progress bar and a cancel button each.
  Several can upload at once; they join the queue in the order dropped,
  each as soon as it is ready.
- The server plays uploaded songs itself. Pause, resume, seek, skip,
  vote skip, repeat, shuffle, history, "Add again" and the now-playing
  position behave as they do for Lavalink tracks.
- Works on a server with no Lavalink node; on one with Lavalink, uploaded
  songs are still played by the server, never by Lavalink.
- Works when voice runs on external voice nodes: the server streams the
  encoded audio to the node serving the channel.
- Local songs show in the player's search, marked **Local**. With Lavalink
  available they come after Lavalink's results; without it they are the
  only results. Members search only their own songs unless a role gives
  them **Access all uploaded songs** (§5.7).
- Members remove their own songs from the server by right-clicking them;
  only Administrators can remove anyone's.

Non-goals

- Playlists or albums of local songs; folders dropped onto the player.
- Editing a local song's title or artist after upload.
- Playing local songs from chat commands (`/play` keeps searching
  Lavalink; it may be added later).
- Downloading a local song from the server.
- A "choose file" button: songs come in by dragging (the reviewer's call;
  easy to add later).
- Volume, equaliser or other filters (the player has none today).

Anti-goals (a failure even if it works)

- The same file stored twice on one server.
- Lavalink ever fetching an uploaded file.
- An uploaded file left to the attachment clean-up, which deletes uploads
  that never became a message.
- Audio that stutters for listeners because decoding falls behind, or a
  bad file that brings the server down.
- A server with no Lavalink still saying music needs a node.

## 3. Existing pieces to build on

- **Uploads** (`docs/attachments.md`, `crates/opencord-server/src/attachments/`):
  `CreateUpload` → piece-by-piece `PUT /media/uploads/{id}` →
  `POST …/finish`, with the client core's upload manager reporting
  progress and `CancelUpload` stopping one. Files are encrypted at rest on
  local disk or S3. Size limits come from `[uploads] max_file_mb` and role
  limits. Music uploads reuse all of this.
- **Music sessions** (`crates/opencord-server/src/music/session.rs`): one
  task per voice channel owning the queue, history, repeat, shuffle, vote
  skip and the play state; today it drives a Lavalink player.
- **The voice node's music input** (`VoiceNode::music_frame(channel_id,
  MusicPacket)`): takes Opus frames and sends them to the channel's
  listeners as its music stream. Lavalink's audio enters here through the
  bridge; the server's own player enters here too.
- **External voice nodes** (`crates/opencord-server/src/voice_node/`) take
  orders from the main server over a control channel (`NodeCommand`).
- **Audio code**: `symphonia` (MP3, FLAC, AAC/M4A, Ogg Vorbis/Opus, WAV),
  `rubato` and `opus` are already workspace dependencies; `sha2` too. The
  soundboard already keeps files by SHA-256.
- **Artwork proxy** (`music/artwork.rs`) for cover images.

## 4. The owner's decisions

- The Opencord server plays uploaded songs itself, not Lavalink, and it
  must work on servers with no Lavalink.
- Re-uploads are caught by the file's hash; a known file plays at once.
- Upload progress shows, and an upload can be cancelled.
- Uploaded songs show in search as local and can be queued from there,
  listed after Lavalink's results when Lavalink is available.
- With voice on external nodes, the server streams the audio to the node.
- A new permission, **Access all uploaded songs**, off by default: with it
  a member searches every local song; without it, only their own.
- A member can remove a song from the server only if they uploaded it,
  by right-clicking it. Access all uploaded songs does not let them remove
  anyone else's; only Administrators can.

## 5. How it works

### 5.1 Dropping files

1. The member drops files from the desktop onto the music player (the
   panel, its queue or the cover). Files that are not audio by extension
   (mp3, flac, m4a, aac, ogg, oga, opus, wav) are refused with a short
   note; the rest go on.
2. For each file the app's core computes SHA-256 while reading it once,
   showing "Checking…" on the file's row.
3. It sends `MusicFindLocal { channel_id, sha256, size_bytes }`. If the
   server has the song, it is queued (`MusicEnqueue` with a local song id)
   and the row disappears. Nothing is uploaded.
4. Otherwise the app uploads it through the existing upload pipeline, with
   `CreateUpload` marked as a music upload (a new purpose; no message is
   made from it), showing progress and a cancel button.
5. At finish the server checks the file (§5.3). If it is good, it becomes
   a local song and is queued in the channel it was dropped on, at the end,
   by the member who dropped it. If another upload of the same bytes
   finished first, this one is discarded and the existing song is queued.
6. Failures show on the row with the reason (not audio, too big, no space,
   no permission) and a dismiss button.

Files dropped on the cover play now instead of joining the end, as a
dropped search result does today; that needs MUSIC_MANAGE_QUEUE, and
without it they join the end.

The upload rows sit above the queue in the music panel. Uploads go on if
the member leaves the channel; the song still joins the queue it was meant
for, if that channel's player still exists.

### 5.2 Local songs

A new table, `music_local_songs`: id, sha256 (unique), size, the stored
file's reference in upload storage, original filename, title, artist,
duration, artwork, first uploader id, created at. A second table,
`music_local_song_owners (song_id, user_id, added_at)`, lists who has the
song as their own: whoever uploaded it, and anyone who later dropped the
same file (their drop matched the hash, so they have it too; the reviewer's
call). A song with no owners left is deleted. The file is kept by the
upload storage backend under a kind of its own, so attachment clean-up
never touches it.

Title and artist come from the file's tags; without them, the title is
the filename without its extension and the artist is the uploader's name.
Embedded cover art is kept and served through the artwork proxy.

### 5.3 Checks at finish

The server probes the whole file with symphonia before accepting it: it
must decode as one of the supported formats, have an audio track, and last
at least one second. The probe runs on a blocking thread with a time
limit, so a malformed file cannot stall the server. The duration comes
from decoding, not from what the client says.

### 5.4 The server's own player

Each queue item is either a Lavalink track or a local song. The music
session picks the engine per item:

- A Lavalink track plays as today.
- A local song plays through a new **built-in player**: a task that reads
  the stored file, decodes it, resamples to 48 kHz stereo, encodes 20 ms
  Opus frames, and paces them in real time into the voice node's music
  input (or to an external node, §5.5). It keeps a few frames buffered
  ahead, so listeners hear no gaps. It reports position and the end of the
  track to the session as Lavalink's events do.
- Pause stops sending frames; seek decodes from the new position; skip and
  the next track stop the task. Switching between a Lavalink track and a
  local song hands over cleanly: one engine stops before the other starts.
- A session whose queue holds only local songs needs no Lavalink node, and
  never shows "waiting for a node". A Lavalink track in the queue of a
  server without nodes waits as today, while local songs ahead of it play.

Decoding and encoding run on blocking threads; frames are paced from an
async task. One player task per channel playing a local song.

### 5.5 External voice nodes

When the channel is served by an external voice node, the built-in player
still runs on the main server and sends its Opus frames to that node over
the control channel, as a new `NodeCommand` carrying music frames for a
channel. The node hands them to its own music input. No file ever goes to
the node.

### 5.6 Search

`MusicSearch` also searches local songs: title, artist and filename,
case-insensitive, word by word. Results carry the source `local`, shown as
**Local**, with their own result ids, and are queued like any result
(including "Play now" by dropping on the cover). When Lavalink returns
results, local ones follow them; with no Lavalink node, the search returns
local songs only and does not report missing nodes as an error. A link
pasted into search never matches local songs.

### 5.7 Permissions and limits

- Uploading a song needs MUSIC_ADD and ATTACH_FILES in the voice channel,
  and the file must be within the member's upload size limit.
- Queuing a local song (from a drop, from search, or "Add again" from the
  channel's history) needs MUSIC_ADD, like any track.
- **Access all uploaded songs** (`MUSIC_ALL_LOCAL_SONGS`, bit 32, a
  server-wide permission in the Music group of the role editor; off for
  @everyone and for new roles): search covers every local song. Without
  it, search covers only the songs the member owns (§5.2). Songs others
  play in the channel still show in its queue and history as usual, and
  "Add again" works on them (the reviewer's call: they were heard there).
- **Removing a song from the server**: a member removes a song they own by
  right-clicking it (a Local search result, or a queue or history item)
  and choosing **Remove from server**, after a confirmation. This drops
  them from its owners; when no owner is left the file is deleted. Access
  all uploaded songs does not let them remove other people's songs.
  Administrators (ADMINISTRATOR) can remove any song, for everyone, from
  the same menu or from Server settings → Music.
- Local songs count towards the server's storage like attachments.

### 5.8 Managing local songs

Server settings → Music gets a **Local songs** list, for Administrators
only: title, artist, length, size, who uploaded it and when, with a search
box and a remove button. Removing deletes the file for everyone. Whenever a
song's file is deleted, queue items for it are skipped with a notice when
their turn comes ("This song was removed from the server").

### 5.9 Music without Lavalink

Today the player tells members music needs a Lavalink node when there is
none. With this, a server with `[music] enabled = true` and no node shows
the full player: dropping files and searching local songs work, and only
Lavalink searches say no node is available.

## 6. Protocol (outline)

- `MusicTrack.source = "local"`, `source_display_name = "Local"`.
- `MusicFindLocal { channel_id, sha256, size_bytes }` → `optional
  local_song_id`.
- `CreateUpload` gains a purpose (music); a finished music upload creates
  a local song and is enqueued in `channel_id`.
- `MusicEnqueue` gains `repeated int64 local_song_ids`.
- `FetchLocalSongs { query, before_id }` (ADMINISTRATOR) and a
  `LocalSongs` reply, for the settings list.
- `RemoveLocalSong { id }`: an owner drops their ownership; an
  Administrator deletes the song for everyone.
- `MusicTrack` gains whether the caller may remove the song, so the app
  knows whether to offer **Remove from server**.
- The new permission bit, `MUSIC_ALL_LOCAL_SONGS = 1 << 32`.
- `NodeCommand` gains music frames for a channel (§5.5).

The builder chooses field numbers and the exact shapes, following
`docs/protocol.md`'s rules.

## 7. What is left to the builder

- Where the music upload purpose lives in `CreateUpload`.
- The built-in player's buffer size and pacing, within "no audible gaps".
- How a seek in a format with no index (some MP3s) finds its place.
- Exact copy for the upload rows and errors, in the app's usual voice.

## 8. Checks

- Server tests: a known hash queues without upload; a race of two uploads
  of the same bytes keeps one song; a non-audio file is refused at finish;
  the session plays a local song with no Lavalink node, pauses, seeks and
  moves to the next item; a mixed queue hands over between engines;
  search orders local results after Lavalink's; without Access all
  uploaded songs search finds only one's own songs, with it every song;
  a second dropper of the same file becomes an owner and finds it; an
  owner's remove keeps the song for its other owners and deletes it after
  the last; a member with Access all uploaded songs cannot remove another's
  song, an Administrator can; attachment clean-up leaves local songs alone;
  deleting a song skips its queue items.
- Core tests: hashing, the find-then-upload flow, cancel.
- App tests: drop → rows with progress and cancel; a known file queued
  without a progress bar; a refused file; Local in search; Remove from
  server offered only where allowed; the permission in the role editor;
  the settings list for Administrators.
- By hand: a server with no Lavalink plays an MP3 and a FLAC for two
  listeners; the same file dropped again queues at once; an external voice
  node carries a local song.
