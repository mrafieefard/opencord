# Files, storage and voice messages

Members attach files and voice messages to messages. The server keeps every file encrypted, on its own disk or in an S3-compatible bucket, and hands it only to members who can read the channel, inside the app. This page is for the people who run a server. How the app sends and the server checks each request is in [protocol.md](protocol.md#attachments).

- [Where files go](#where-files-go)
- [Encryption and secrets.key](#encryption-and-secretskey)
- [Limits and blocked extensions](#limits-and-blocked-extensions)
- [Only in the app](#only-in-the-app)
- [Moving between storages](#moving-between-storages)
- [Testing](#testing)

## Where files go

Files arrive in pieces into `uploads` in the data directory, then go to the **active storage**. A server always has its own storage, and the owner can add buckets under **Server settings → Upload settings → Storage** and make one of them active. New files go to the active storage; each file stays where it was stored until it is [moved](#moving-between-storages).

### This server's own storage

Files go to `attachments` in the data directory, or wherever `local_dir` says:

```toml
[uploads]
# OPENCORD_UPLOADS_LOCAL_DIR
local_dir = "/mnt/storage/opencord-attachments"
```

`uploads` must have room for the files being uploaded: the server refuses an upload (`STORAGE_FULL`) when the folder has less free space than the file plus 512 MiB. Upload settings shows the room left.

### S3-compatible buckets

**Connect a bucket** asks for a provider, which fills in the endpoint, region and path style, then the bucket, an optional key prefix, and an access key and its secret. **Test** writes a small file, reads part of it back, uploads a file in parts and deletes the test files; **Save** stays off until the test passes on the settings shown. The secret is sealed with [secrets.key](#encryption-and-secretskey) and never sent back: the page shows only the access key's last four characters. Editing a bucket keeps its saved keys when both are left empty; a new access key needs its secret.

| Provider | Endpoint | Region | Path style |
|---|---|---|---|
| Amazon S3 | `https://s3.<region>.amazonaws.com` | the bucket's, for example `eu-central-1` | off |
| Cloudflare R2 | `https://<account id>.r2.cloudflarestorage.com` | `auto` | on |
| Backblaze B2 | `https://s3.<region>.backblazeb2.com` | the bucket's, for example `us-west-004` | off |
| Wasabi | `https://s3.<region>.wasabisys.com` | the bucket's, for example `eu-central-1` | off |
| MinIO / Custom | the server's address | its region, often `us-east-1` | on |

- **Amazon S3.** Create a private bucket, then an IAM user with only this policy (replace `BUCKET`), and an access key for it:

  ```json
  {"Version": "2012-10-17", "Statement": [{"Effect": "Allow", "Action": ["s3:PutObject", "s3:GetObject", "s3:DeleteObject", "s3:AbortMultipartUpload", "s3:ListMultipartUploadParts"], "Resource": "arn:aws:s3:::BUCKET/*"}, {"Effect": "Allow", "Action": ["s3:ListBucket"], "Resource": "arn:aws:s3:::BUCKET"}]}
  ```

- **Cloudflare R2.** Create a bucket, then under **Manage R2 API tokens** a token with **Object Read & Write** for that bucket only. Its Access Key ID and Secret Access Key go in the dialog, and the account ID (on the R2 overview page) in **Account ID**.
- **Backblaze B2.** Create a private bucket; its endpoint is on the bucket's page. Under **Application Keys**, add a key that can access only this bucket, with read and write. The key ID is the access key ID, the application key the secret.
- **Wasabi.** Create a bucket, then a user with programmatic access and a policy like Amazon's above (Wasabi takes the same JSON), and an access key for it.
- **MinIO and other servers.** A user with read and write on the bucket. A plain `http://` endpoint needs **Allow plain HTTP**: the keys and files then cross the network unencrypted, so use it only on a network you trust.

Files go up in one request up to 64 MiB and in 16 MiB parts above that.

## Encryption and secrets.key

Every file, and every thumbnail, is encrypted on the server with its own random key (XChaCha20-Poly1305, in 64 KiB segments) before it is written anywhere. Each file's key is sealed with the server's **secrets key**, `secrets.key` in the data directory unless `secrets_file` says otherwise. The same key seals the bucket secrets, certificates and tokens the server keeps.

What this protects:
- a bucket that leaks, or a provider that looks;
- a copy of the attachments folder;
- a backup of the data directory made without `secrets.key`.

What it doesn't:
- someone with the whole disk, `secrets.key` included. To protect files against a stolen disk, keep the key on another disk, or somewhere your system provides at start (below);
- someone who controls the running server.

**Back up `secrets.key`, apart from the files.** Without it, no stored file and no sealed setting can be read again, and nothing can recover them.

To move the key:
1. stop the server;
2. move the file;
3. set `secrets_file` (or `OPENCORD_SECRETS_FILE`) to its new place;
4. start the server.

```toml
[server]
# OPENCORD_SECRETS_FILE
secrets_file = "/run/secrets/opencord-secrets.key"
```

A server with no key file makes one on first start. When the database already holds data sealed with a key and the file is missing, the server refuses to start, rather than make a new key that opens nothing:

```
the secrets key <path> is missing, but the database holds data sealed with it: restore the file or point [server] secrets_file at it
```

Restore the file from a backup, or point `secrets_file` at it. A key file of the wrong size stops the server with "`<path>` is not a 32-byte key".

The Docker and systemd files in [deploy/](../deploy) show how to hand the key to the server from outside its data directory.

## Limits and blocked extensions

- **File size.** `max_file_mb` (default 4096, 1 to 102400; 1 MB = 1,048,576 bytes) is the owner's limit and the ceiling for everyone else. Each role can have an upload limit (**Server settings → Roles**), and a member gets the largest of @everyone's and their roles' limits, never above the ceiling. @everyone starts at 50 MB and always has a limit; a role's limit is 1 MB or more. Someone who manages roles can't give a role a larger limit than their own.

  ```toml
  [uploads]
  # OPENCORD_UPLOADS_MAX_FILE_MB
  max_file_mb = 4096
  ```

- **Per message:** at most 10 files. A voice message is sent on its own, without text.
- **At a time:** each member can have at most 10 uploads not yet sent, and start 20 a minute. The app uploads a file as soon as it is attached, so files waiting in the message box count; removing one cancels its upload. An upload left unfinished, or finished and never sent, is deleted after a day; a failed one after an hour.
- **Blocked extensions** (**Server settings → Upload settings**): uploads whose name ends with one of them are refused (`FILE_TYPE_BLOCKED`). Nothing is blocked on a new server. **Block programs** adds the common program and script extensions (`exe`, `msi`, `bat`, `ps1`, `js`, `jar`, `apk`, `dmg`, `deb`, `sh` and others). Up to 100 entries of up to 20 characters; an entry can hold inner dots (`tar.gz`).
- **Voice messages** last at most 30 minutes, on every server. A recording stops there and waits, unsent, for the member to send or delete it. The server checks the length and computes the waveform itself.
- **Pictures** are checked and get a thumbnail (at most 800 pixels on a side) when they are at most 100 MiB, 16,384 pixels on a side and 50 megapixels. A picture beyond that, or one that doesn't decode, stays a file people can download. Videos get the thumbnail the sender's app made from their first frame.

Permissions: **Attach files** to send files, **Send voice messages** for voice messages, and **Read message history** in the channel to open either.

## Only in the app

Files are never public:
- there are no links to them: the app fetches each file with the member's own token, valid for a day, and the server checks, every time, that the message still exists and that the member can see the channel and read its history;
- the server sends them with `Cache-Control: private, no-store`, as downloads, never as pages a browser would show;
- the app keeps what it has shown in its own cache (at most 1 GiB, files up to 20 MB, thumbnails always), encrypted with a key kept in the system's keychain.

What it can't do: a member who can open a file can save a copy (**Download**), and what they do with it after that is up to them. Deleting a message deletes its files; copies already saved stay.

## Moving between storages

Making another storage active changes only where new files go. To move the old ones, **Move files** under Storage in Upload settings copies every file stored elsewhere to the active storage, two at a time. They are copied encrypted, as they are, so the server never decrypts them on the way, and each file switches over once its copy is complete. Progress shows on the page. **Stop** stops after the files in flight; starting again continues with what is left. A file that fails three times stays where it was and is counted as failed.

The old copies are deleted an hour after each switch, so downloads that started before it can finish. While a move runs, no other storage can be activated: finish or stop it first. A storage can be removed once it is not active, holds no files and has nothing left waiting to be deleted; the server's own storage can't be removed.

## Testing

### The S3 tests

The S3 tests run against RustFS, an S3-compatible server, in Docker:

```bash
docker run -d --rm --name opencord-s3 -p 9000:9000 \
  -e RUSTFS_ACCESS_KEY=opencord -e RUSTFS_SECRET_KEY=opencord-secret \
  rustfs/rustfs:1.0.3 /data
cargo test -p opencord-server --test s3 -- --ignored --test-threads=1
docker stop opencord-s3
```

### Transfer speed

`app/tool/check_transfer.sh` sends a 1 GiB file to a release server on this machine, saves it back, and prints both speeds and how much the server's memory grew ([performance.md](performance.md#media-uploads-a8)).

### By hand, on a desktop

What the automated tests can't see:

- **Sending:** attach several files by the button, by dropping them and by pasting a picture; one with a blocked extension and one over the limit are refused before uploading; cancel one mid-upload; turn the network off mid-upload and on again, and the upload continues.
- **Showing:** pictures and GIFs in the message, the viewer (zoom, ←/→, Copy image, Download), a text file's preview, and a file card's Download.
- **Storage:** connect a bucket, test it, activate it, send a file, move the old files, and remove the old bucket.
- **Voice messages:** hold to record and release to send; slide left to cancel; slide up to lock, then preview and send; play one at each speed and seek; record during a call, and play one during a call: the others hear neither.
- **Video and audio:** play an MP4, a WebM and a phone video recorded upright; seek; change the volume; play one during a call; start a voice message while a video plays, then go back to the video.
