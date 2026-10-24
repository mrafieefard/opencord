import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/repository/events.dart';

export 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/model/media_handle.dart';
export 'package:opencord/core/model/media_handle.dart';
import 'package:opencord/core/model/private.dart';
export 'package:opencord/core/model/private.dart';
import 'package:opencord/core/model/voice_message.dart';
export 'package:opencord/core/model/voice_message.dart';

/// UI features that need protocol support Phase 1 does not have yet. The
/// mock has them all; the Rust core reports what it can do, and the UI
/// hides the rest (docs/decisions.md D13).
@immutable
class RepoCapabilities {
  const RepoCapabilities({
    this.reactions = false,
    this.pins = false,
    this.voice = false,
    this.camera = false,
    this.screenShare = false,
    this.announcementChannels = false,
    this.readStates = false,
    this.replies = false,
    this.soundboard = false,
    this.ownScreenPicker = false,
    this.lowerOtherApps = false,
  });

  static const everything = RepoCapabilities(
    reactions: true,
    pins: true,
    voice: true,
    camera: true,
    screenShare: true,
    announcementChannels: true,
    readStates: true,
    replies: true,
    soundboard: true,
    lowerOtherApps: true,
  );

  final bool reactions;
  final bool pins;
  final bool voice;
  final bool camera;
  final bool screenShare;
  final bool announcementChannels;

  /// Unread counts that survive restarts.
  final bool readStates;

  /// Messages can answer an earlier one.
  final bool replies;

  /// Soundboard sounds play and can be uploaded (Phase 2 V8).
  final bool soundboard;

  /// The share dialog lists the screens and windows itself
  /// ([OpencordRepository.screenSources]); otherwise the system's picker
  /// opens after it (Phase 2 plan §9.1).
  final bool ownScreenPicker;

  /// Other apps can be lowered while people talk (Phase 2 §12); not on
  /// macOS.
  final bool lowerOtherApps;
}

/// The current user as far as the UI is concerned.
@immutable
class LocalIdentity {
  const LocalIdentity({
    required this.displayName,
    required this.fingerprint,
    this.publicKeyHex = '',
  });

  final String displayName;

  /// Like `ABCD-EFGH-IJKL-MNOP`.
  final String fingerprint;

  /// The Ed25519 public key, lowercase hex.
  final String publicKeyHex;
}

/// A freshly made identity, shown before it is used (Phase 1 §9.1): its
/// fingerprint and the backup text that holds its secret.
@immutable
class NewIdentity {
  const NewIdentity({
    required this.fingerprint,
    required this.publicKeyHex,
    required this.backup,
  });

  final String fingerprint;
  final String publicKeyHex;
  final String backup;
}

sealed class AddServerResult {
  const AddServerResult();
}

final class ServerAdded extends AddServerResult {
  const ServerAdded(this.server);

  final ServerSummary server;
}

/// The certificate is not pinned and not from a public authority: ask the
/// user, then call [OpencordRepository.trustFingerprint] and add again.
final class ServerNeedsTrust extends AddServerResult {
  const ServerNeedsTrust({required this.address, required this.fingerprint});

  final String address;

  /// Lowercase hex SHA-256 of the certificate.
  final String fingerprint;
}

enum RepoErrorKind {
  /// The camera: refused by the user or the system, missing, unusable, not
  /// supported here yet, or full in this channel (Phase 2 V5).
  cameraDenied,
  cameraMissing,
  cameraUnsupported,
  cameraLimit,

  /// Screen sharing: the picker closed without a choice, refused, not
  /// supported here yet; above the server's maximum; a full stream (Phase 2
  /// V6).
  screenCancelled,
  screenDenied,
  screenUnsupported,
  qualityLimit,
  streamFull,

  /// The soundboard (Phase 2 V8): cooling down or three sounds already
  /// playing; a file too long or not a sound; no room for another.
  soundCooldown,
  soundTooLong,
  soundInvalid,
  soundboardFull,

  /// Music (Phase 3): the track changed before the action arrived; not
  /// set up or no node to play.
  musicStale,
  musicUnavailable,

  /// Media uploads (Phase 4):
  /// A file larger than the member's or the server's limit.
  fileTooLarge,

  /// A file whose extension is on the server's blocked list.
  fileTypeBlocked,

  /// A voice message that is not Opus or has bad metadata.
  voiceMessageInvalid,

  /// A voice message longer than the server allows.
  voiceMessageTooLong,

  /// Storage unreachable, bucket test failed, or active storage missing.
  storageUnavailable,

  /// No space left on the storage backend.
  storageFull,

  /// The upload, download, or operation was cancelled.
  cancelled,

  /// The target is not accepting friend requests (§6.7).
  friendRequestsClosed,

  /// The target only accepts messages from friends (§6.7).
  messageRequestsClosed,

  /// The message or friend request could not be delivered (§6.7).
  undeliverable,

  /// Private messages are unavailable or not relayed on this server (§6.7).
  privateMessagesUnavailable,

  unauthorized,
  forbidden,
  notFound,
  invalidArgument,
  rateLimited,
  conflict,
  voiceChannelFull,
  notConnected,
  timeout,
  connection,
  fingerprintMismatch,
  rejected,
  other,
}

/// A failed request, with a message fit for showing to the user.
class RepoException implements Exception {
  const RepoException(this.kind, this.message, {this.retryAfter});

  final RepoErrorKind kind;
  final String message;

  /// Set when rate limited.
  final Duration? retryAfter;

  @override
  String toString() => message;
}

/// Self presence choices; [invisible] shows as offline to everyone else.
enum SelfPresence { online, idle, doNotDisturb, invisible }

extension SelfPresenceShape on SelfPresence {
  Presence get shown => switch (this) {
    SelfPresence.online => Presence.online,
    SelfPresence.idle => Presence.idle,
    SelfPresence.doNotDisturb => Presence.doNotDisturb,
    SelfPresence.invisible => Presence.offline,
  };

  String get label => switch (this) {
    SelfPresence.online => 'Online',
    SelfPresence.idle => 'Idle',
    SelfPresence.doNotDisturb => 'Do not disturb',
    SelfPresence.invisible => 'Invisible',
  };
}

/// Everything the UI asks of the outside world. Implemented by the mock
/// (plan §11) and by the Rust core.
abstract interface class OpencordRepository {
  RepoCapabilities get capabilities;

  /// Every event from every server, in order.
  Stream<RepoEvent> get events;

  /// Connects to the saved servers; events start flowing.
  void start();

  void dispose();

  LocalIdentity? get identity;

  /// Why the saved identity could not be read (the system keyring is
  /// missing or locked), or null. Nothing can go on until it can.
  RepoException? get identityUnavailable;

  /// Reads the saved identity again, after [identityUnavailable].
  Future<void> reloadIdentity();

  List<ServerSummary> get servers;

  Future<AddServerResult> addServer(String linkOrAddress, {String? claimToken});

  Future<void> trustFingerprint(String address, String fingerprint);

  Future<void> removeServer(String serverKey);

  /// Skips the backoff wait and tries to connect right away.
  Future<void> retryNow(String serverKey);

  Future<List<Message>> fetchMessages(
    String serverKey,
    int channelId, {
    int? before,
    int limit = 50,
  });

  Future<Message> sendMessage(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    int? replyToId,
  });

  Future<Message> editMessage(
    String serverKey,
    int channelId,
    int messageId,
    String content,
  );

  Future<void> deleteMessage(String serverKey, int channelId, int messageId);

  Future<void> startTyping(String serverKey, int channelId);

  Future<void> toggleReaction(
    String serverKey,
    int channelId,
    int messageId,
    String emoji,
  );

  Future<void> setPinned(
    String serverKey,
    int channelId,
    int messageId, {
    required bool pinned,
  });

  /// A channel's pinned messages; empty without [RepoCapabilities.pins].
  Future<List<Message>> fetchPins(String serverKey, int channelId);

  Future<Channel> createChannel(
    String serverKey, {
    required ChannelKind kind,
    required String name,
    int? parentId,
    String? topic,
    bool private = false,
  });

  /// Changes what is given; a [parentId] of 0 moves the channel out of
  /// its category, an empty [topic] clears it. [bitrate] (bits per
  /// second), [userLimit] and [textInVoice] are for voice channels.
  Future<Channel> updateChannel(
    String serverKey,
    int channelId, {
    String? name,
    String? topic,
    int? parentId,
    int? bitrate,
    int? userLimit,
    bool? textInVoice,
  });

  Future<void> deleteChannel(String serverKey, int channelId);

  /// New positions, as `channelId → position`.
  Future<void> reorderChannels(String serverKey, Map<int, int> positions);

  Future<void> setOverwrite(
    String serverKey,
    int channelId,
    PermissionOverwrite overwrite,
  );

  Future<void> deleteOverwrite(
    String serverKey,
    int channelId,
    OverwriteTargetKind kind,
    int targetId,
  );

  Future<Role> createRole(
    String serverKey, {
    required String name,
    Permissions permissions = Permissions.none,
    bool hoist = false,
    bool mentionable = false,
    int? maxUploadBytes,
  });

  Future<Role> updateRole(
    String serverKey,
    int roleId, {
    String? name,
    Permissions? permissions,
    bool? hoist,
    bool? mentionable,
    int? maxUploadBytes,
  });

  Future<void> deleteRole(String serverKey, int roleId);

  /// The listed roles take the positions they already hold, from lowest to
  /// highest in list order.
  Future<void> reorderRoles(String serverKey, List<int> roleIds);

  Future<void> addMemberRole(String serverKey, int userId, int roleId);

  Future<void> removeMemberRole(String serverKey, int userId, int roleId);

  Future<void> kickMember(String serverKey, int userId, {String? reason});

  Future<void> banMember(String serverKey, int userId, {String? reason});

  Future<void> unbanMember(String serverKey, int userId);

  Future<List<Ban>> fetchBans(String serverKey);

  Future<void> updateNickname(String serverKey, int userId, String? nickname);

  Future<void> updateDisplayName(String displayName);

  /// This person's profile picture's hash on this device; null without one.
  String? ownAvatarHash();

  /// This person's profile picture's file on this device; null without one.
  String? ownAvatarFile();

  /// Makes this person's profile picture from the image at [path], or
  /// removes it (null), and sends it to every server, as the display name
  /// is: servers not connected now get it after their next Ready.
  Future<void> setAvatar(String? path);

  /// The server's icon, made from the image at [path], or none. Manage
  /// server.
  Future<void> setServerIcon(String serverKey, String? path);

  /// The file of the profile picture or server icon named [hash].
  Future<String> avatarPath(String serverKey, String hash);

  /// The identity's secret key as a backup text to keep safe (§8.1).
  Future<String> exportIdentityBackup();

  /// Replaces this device's identity with one from a backup.
  Future<void> importIdentityBackup(String backup);

  /// A new identity for onboarding; nothing is saved until
  /// [adoptIdentity].
  Future<NewIdentity> generateIdentity();

  /// Makes the identity in [backup] this device's, under [displayName],
  /// and connects to the saved servers (Phase 1 §9.1).
  Future<void> adoptIdentity(String backup, {required String displayName});

  /// The newest message read in a channel, for servers that do not keep
  /// read states themselves (§6).
  void markRead(String serverKey, int channelId, int messageId);

  Future<void> updatePresence(SelfPresence presence);

  /// Through [domainId]'s domain, or the primary one when the server has
  /// one (Phase 3 plan §7.6).
  Future<Invite> createInvite(
    String serverKey, {
    Duration? expiresIn,
    int? maxUses,
    int? domainId,
  });

  Future<List<Invite>> fetchInvites(String serverKey);

  Future<void> revokeInvite(String serverKey, String code);

  Future<void> updateServer(
    String serverKey, {
    String? name,
    String? description,
    bool? openJoin,
  });

  /// Saves the server's voice, video and soundboard settings (Phase 2
  /// plan §5.2); needs Manage server.
  Future<void> updateVoiceSettings(String serverKey, VoiceSettings settings);

  /// Domains & network (Phase 3 plan §8), the owner's only. While watched,
  /// changes arrive as [NetworkStatusChanged].
  Future<NetworkInfo> watchNetwork(String serverKey, {required bool watch});

  /// How the public address is found: detected, or set by hand (an empty
  /// override is none).
  Future<NetworkInfo> setPublicAddress(
    String serverKey, {
    required bool autoDetect,
    String overrideIpv4 = '',
    String overrideIpv6 = '',
  });

  /// Looks the public address up now.
  Future<NetworkInfo> detectPublicAddress(String serverKey);

  Future<NetworkInfo> updateNetworkSettings(
    String serverKey,
    NetworkSettingsChanges changes,
  );

  /// Attaches a domain, pending its check (Phase 3 plan §6).
  Future<NetworkInfo> addDomain(
    String serverKey, {
    required String hostname,
    required DomainKind kind,
    int? publicPort,
  });

  Future<NetworkInfo> updateDomain(
    String serverKey,
    int domainId,
    DomainUpdate update,
  );

  Future<NetworkInfo> deleteDomain(String serverKey, int domainId);

  /// Starts a check; its result arrives as [NetworkStatusChanged].
  Future<NetworkInfo> checkDomain(String serverKey, int domainId);

  /// A domain's last checks, newest first.
  Future<List<DomainCheck>> domainChecks(String serverKey, int domainId);

  /// Starts getting a Let's Encrypt certificate; progress arrives as
  /// [NetworkStatusChanged]. [agreeTerms] is needed for the first one.
  Future<NetworkInfo> requestCertificate(
    String serverKey,
    int domainId, {
    String? email,
    bool agreeTerms = false,
  });

  /// The owner's own certificate chain and key, in PEM.
  Future<NetworkInfo> uploadCertificate(
    String serverKey,
    int domainId, {
    required String chainPem,
    required String keyPem,
  });

  /// A free subdomain from DuckDNS or deSEC (plan §9.1): the server points
  /// it here, keeps it pointed here, checks it and gets its certificate. An
  /// empty [token] keeps the one connected.
  Future<NetworkInfo> addFreeSubdomain(
    String serverKey, {
    required DnsProviderKind provider,
    required String name,
    String token = '',
    String? email,
    bool agreeTerms = false,
  });

  /// Starts a Cloudflare Tunnel (plan §9.2); a named one needs its [token]
  /// (empty keeps the saved one) and [hostname].
  Future<NetworkInfo> startTunnel(
    String serverKey,
    TunnelKind kind, {
    String token = '',
    String hostname = '',
  });

  Future<NetworkInfo> stopTunnel(String serverKey);

  /// cloudflared's last lines of output.
  Future<List<String>> tunnelLog(String serverKey);

  /// Has the server download cloudflared, checked by its checksum.
  Future<NetworkInfo> installCloudflared(String serverKey);

  /// Connects a DNS provider; an empty [token] disconnects it.
  Future<NetworkInfo> setDnsProvider(
    String serverKey,
    DnsProviderKind kind,
    String token,
  );

  /// The Lavalink nodes (Phase 3 plan §12); needs Manage server. While
  /// watched, changes arrive as [LavalinkNodeChanged] and
  /// [LavalinkNodeEventReceived].
  Future<LavalinkNodes> lavalinkNodes(String serverKey, {required bool watch});

  Future<LavalinkNodes> createLavalinkNode(
    String serverKey,
    LavalinkNodeSettings settings,
    String password,
  );

  Future<LavalinkNodes> updateLavalinkNode(
    String serverKey,
    int nodeId,
    LavalinkNodeChanges changes,
  );

  Future<LavalinkNodes> deleteLavalinkNode(String serverKey, int nodeId);

  /// Starts the connection test; its steps arrive as
  /// [LavalinkTestProgressed].
  Future<void> testLavalinkNode(String serverKey, int nodeId);

  /// Moves the channels on a node now: to [toNodeId], or each to the best
  /// other node. Needs Manage server.
  Future<LavalinkNodes> moveLavalinkPlayers(
    String serverKey,
    int fromNodeId, {
    int? toNodeId,
  });

  /// The music settings (Phase 3 plan §16.2). Changes arrive as
  /// [MusicSettingsChanged].
  Future<MusicSettings> musicSettings(String serverKey);

  /// Saves what differs from [before] in [after]. Needs Manage music.
  Future<MusicSettings> updateMusicSettings(
    String serverKey,
    MusicSettings before,
    MusicSettings after,
  );

  /// Members blocked from music. Needs Manage music.
  Future<List<MusicBlock>> musicBlocks(String serverKey);

  /// Blocks a member from music, or unblocks them. Needs Manage music.
  Future<List<MusicBlock>> blockFromMusic(
    String serverKey,
    int userId, {
    required bool blocked,
    String reason = '',
  });

  /// Searches, or loads a link, for a voice channel's music (Phase 3 plan
  /// §15); [source] empty for the server's default. Needs Add music.
  Future<MusicSearchResults> musicSearch(
    String serverKey,
    int channelId,
    String query, {
    String source = '',
  });

  /// The channel's player; its changes come as [MusicSessionChanged] from
  /// now on, even when not connected to the channel (§14.3).
  Future<MusicSession> musicSubscribe(String serverKey, int channelId);

  Future<void> musicUnsubscribe(String serverKey, int channelId);

  /// Adds search results, or recently played tracks by their item ids
  /// ([historyItemIds], "Add again"), at the end, before [beforeItemId],
  /// next or now. Needs Add music; a position, next and now need Manage
  /// the queue.
  Future<MusicSession> musicEnqueue(
    String serverKey,
    int channelId,
    List<String> resultIds, {
    List<String> historyItemIds = const [],
    String? beforeItemId,
    MusicPlacement placement = MusicPlacement.end,
  });

  Future<MusicSession> musicPause(
    String serverKey,
    int channelId, {
    required bool paused,
  });

  /// With [expectedItemId], nothing happens when another track plays by
  /// then ([RepoErrorKind.musicStale]).
  Future<MusicSession> musicSkip(
    String serverKey,
    int channelId, {
    required bool forward,
    String? expectedItemId,
  });

  /// Votes to skip the current track when vote skip is enabled (Phase 3 plan §18.1).
  Future<MusicSession> musicVoteSkip(
    String serverKey,
    int channelId, {
    required String expectedItemId,
  });

  Future<MusicSession> musicSeek(
    String serverKey,
    int channelId,
    Duration position, {
    String? expectedItemId,
  });

  /// Stops the music and clears the queue. Needs Manage music.
  Future<MusicSession> musicStop(String serverKey, int channelId);

  /// Repeat and shuffle, shared by everyone in the channel.
  Future<MusicSession> musicSetMode(
    String serverKey,
    int channelId, {
    MusicRepeat? repeat,
    bool? shuffle,
  });

  /// Puts an item before another, or at the end ([beforeItemId] null).
  Future<MusicSession> musicMove(
    String serverKey,
    int channelId,
    String itemId, {
    String? beforeItemId,
  });

  Future<MusicSession> musicRemove(
    String serverKey,
    int channelId,
    List<String> itemIds,
  );

  Future<MusicSession> musicClear(String serverKey, int channelId);

  /// This person's music volume and mute, on every server; kept on this
  /// device. Muted, the voice node stops sending the music (§19.8).
  void setMusicListening(MusicListening listening);

  /// A track's artwork through the server, 128 or 512 px: the path of a
  /// file on this device, or null when there is none.
  Future<String?> musicArtwork(String serverKey, String artworkId, int size);

  /// Runs a chat command typed in [channelId] (Phase 3 plan §17). An
  /// "only you" reply comes as [EphemeralMessageReceived] with [nonce].
  Future<CommandOutcome> executeCommand(
    String serverKey,
    int channelId,
    String name,
    Map<String, String> options, {
    required String nonce,
  });

  /// Up to 10 suggestions for an option as typed so far.
  Future<List<CommandChoice>> autocompleteCommand(
    String serverKey,
    int channelId,
    String name,
    String option,
    String value,
  );

  /// Every command, on or off, for the Commands page. Needs Manage server.
  Future<CommandList> commandSettings(String serverKey);

  /// Turns [name] on or off, keeps it to channels (an empty list for
  /// every channel), or chooses where results go. Needs Manage server.
  Future<CommandList> updateCommandSettings(
    String serverKey, {
    String name = '',
    bool? enabled,
    List<int>? channelIds,
    bool? resultsPublic,
  });

  Future<void> joinVoice(String serverKey, int channelId);

  Future<void> leaveVoice();

  /// The camera that turning the camera on starts from now on, by its
  /// device id; `null` for the system's. With the camera on, it switches
  /// to it at once; if that one cannot start, the camera goes off and a
  /// [RepoException] says why.
  Future<void> chooseCamera(String? deviceId);

  /// The cameras there are, as (device id, name). On Linux listing them
  /// may ask for camera access first (the Camera portal).
  Future<List<(String, String)>> cameras();

  /// Turning the camera on may fail (a [RepoException] says why); it then
  /// stays off.
  Future<void> setVoiceSelf({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
  });

  /// The tiles showing video now and their sizes in physical pixels; video
  /// not named is neither received nor decoded (Phase 2 plan §6, §7.11).
  void setVideoWants(List<VideoWant> wants);

  /// Goes live in this device's voice channel (Phase 2 plan §9.1): where
  /// the system has a picker it chooses a screen or a window first;
  /// otherwise [sourceId] is one of [screenSources]
  /// ([RepoCapabilities.ownScreenPicker]). Sharing again while live changes
  /// the source and keeps the viewers. The share comes as
  /// [OwnScreenShareChanged]; a [RepoException] says why it could not start
  /// (the picker closed: [RepoErrorKind.screenCancelled]).
  Future<void> startScreenShare(
    ScreenShareQuality quality, {
    bool audio,
    String? sourceId,
  });

  /// The screens and windows the share dialog lists where Opencord has its
  /// own picker; empty where the system picks.
  Future<List<ScreenSource>> screenSources();

  /// A new quality while live, within the server's maximum.
  Future<void> updateScreenShare(ScreenShareQuality quality, {bool audio});

  Future<void> stopScreenShare();

  /// Whether this system can share a screen's sound (Phase 2 plan §10.1).
  Future<ScreenAudioSupport> screenAudioSupport();

  /// The apps with sound playing now, Opencord left out (§10.4).
  Future<List<ScreenAudioApp>> screenAudioApps();

  /// Whose sound this device's window share sends: an app from
  /// [screenAudioApps] by name, or none.
  void setScreenShareAudioFrom(String? app);

  /// How loud someone's screen share sound is on this device, 0–200 %,
  /// apart from their voice (§10.5).
  void setStreamVolume(String serverKey, int userId, int volume);

  void setStreamMute(String serverKey, int userId, bool muted);

  /// Watching is opt-in (§9.5): nothing of a stream comes until then. A
  /// full stream fails with [RepoErrorKind.streamFull].
  Future<void> watchStream(String key);

  Future<void> unwatchStream(String key);

  /// Microphones and speakers the system offers (Phase 2 plan §7.6).
  Future<AudioDeviceList> audioDevices();

  /// Devices, input mode and volumes for voice; at start and on every
  /// change.
  void applyAudio(AudioConfig config);

  /// The push-to-talk key went down or up.
  void setPushToTalk(bool held);

  /// The priority speaker key went down or up.
  void setPrioritySpeaker(bool held);

  /// Report the microphone's level ([InputLevelChanged]) while a meter
  /// shows it.
  void setLevelMeter(bool on);

  /// Hear yourself through the whole voice chain, in voice or not.
  Future<void> setMicTest(bool on);

  /// The noise suppression to start with on a first run: High when this
  /// computer runs it easily, Standard otherwise (Phase 2 plan §7.3).
  Future<NoiseSuppression> recommendedNoiseSuppression();

  /// Binds the hotkeys system-wide where the system allows (Phase 2 plan
  /// §7.13), replacing the ones bound before; says whether they work while
  /// Opencord is in the background. An action may have several keys.
  /// Toggles pressed there arrive as [HotkeyPressed].
  Future<HotkeySupport> setHotkeys(List<HotkeyBinding> bindings);

  /// How loud someone sounds on this device, 0–200 %.
  void setUserVolume(String serverKey, int userId, int volume);

  /// Silences someone on this device only.
  void setUserLocalMute(String serverKey, int userId, bool muted);

  /// Server mute and deafen: for everyone, until lifted. Needs Mute or
  /// Deafen members in their channel and to outrank them (but not on
  /// yourself).
  Future<void> serverMuteMember(String serverKey, int userId, bool muted);

  Future<void> serverDeafenMember(String serverKey, int userId, bool deafened);

  /// Moves someone in voice to another voice channel; needs Move members in
  /// both.
  Future<void> moveMember(String serverKey, int userId, int channelId);

  /// Takes someone out of voice; needs Move members in their channel.
  Future<void> disconnectMember(String serverKey, int userId);

  /// The sounds every client has (Phase 2 plan §11.8).
  List<DefaultSound> get defaultSounds;

  /// Plays a sound for everyone in this device's voice channel (§11.5). A
  /// sound from another server names that server in [from]; this server
  /// gets a copy first if it has none. The server checks again and may
  /// refuse ([RepoErrorKind.soundCooldown], [RepoErrorKind.forbidden]).
  Future<void> playSound(
    String serverKey,
    int channelId,
    String soundRef, {
    ({String serverKey, Sound sound})? from,
  });

  /// Plays a sound on this device only.
  Future<void> previewSound(String serverKey, String soundRef);

  /// The soundboard volume (0–100 %) and "Mute all soundboards".
  void setSoundboardListening({required int volume, required bool muted});

  /// Plays a voice event's sound on this device only (Phase 2 §12).
  void playCue(VoiceCue cue);

  /// "Mute their soundboard": their sounds are not played here.
  void setUserSoundboardMute(String serverKey, int userId, bool muted);

  /// Needs Manage soundboard; an empty [emoji] clears it.
  Future<Sound> updateSound(
    String serverKey,
    int soundId, {
    String? name,
    String? emoji,
    int? volume,
  });

  Future<void> deleteSound(String serverKey, int soundId);

  /// Opens an audio file (MP3, WAV, FLAC, OGG, M4A; at most 10 MB) for the
  /// trim editor (§11.3).
  Future<SoundDraft> openSoundDraft(String path);

  /// Plays a window of a draft here, as it would be uploaded.
  Future<void> previewSoundDraft(SoundWindow window);

  /// Uploads a window of a draft as a new sound; needs Manage soundboard.
  Future<Sound> uploadSound(
    String serverKey,
    SoundWindow window, {
    required String name,
    String? emoji,
  });

  void closeSoundDraft(int draftId);

  // Media uploads and storage (Phase 4) --------------------------------------

  Future<Message> sendMessageWithFiles(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    required List<StagedFile> files,
  });

  void cancelSend(String serverKey, String nonce);

  /// Uploads [file] as soon as it is staged in [channelId]'s composer;
  /// [StagedUploadEvent]s follow. Completes when it is ready to send;
  /// throws when it fails. Sending [file] takes its upload over.
  Future<void> stageUpload(String serverKey, int channelId, StagedFile file);

  /// The staged file was removed: its upload stops, or is cancelled on the
  /// server once finished.
  Future<void> discardStagedUpload(String serverKey, String stagedId);

  Future<Uint8List> attachmentBytes(
    String serverKey,
    int attachmentId, {
    bool thumbnail = false,
  });

  Future<void> downloadAttachment(
    String serverKey,
    int attachmentId,
    String path,
  );

  void cancelDownload(String serverKey, int attachmentId);

  Future<Uint8List?> clipboardImage();

  Future<void> copyImage(Uint8List bytes);

  Future<UploadSettings> updateUploadSettings(
    String serverKey, {
    List<String>? blockedExtensions,
  });

  Future<StorageInfo> storage(String serverKey);

  Future<StorageInfo> addStorage(String serverKey, String name, S3Config s3);

  Future<StorageInfo> updateStorage(
    String serverKey,
    int storageId, {
    String? name,
    S3Config? s3,
  });

  /// Tests a saved storage ([storageId]) or settings ([s3]). With [s3],
  /// [keysFrom] names the saved storage being edited: the keys [s3] leaves
  /// empty are its saved ones.
  Future<StorageTestResult> testStorage(
    String serverKey, {
    int? storageId,
    S3Config? s3,
    int? keysFrom,
  });

  Future<StorageInfo> activateStorage(String serverKey, int storageId);

  Future<StorageInfo> removeStorage(String serverKey, int storageId);

  Future<StorageInfo> startStorageMove(String serverKey);

  Future<StorageInfo> stopStorageMove(String serverKey);

  Future<int> mediaCacheSize();

  Future<void> clearMediaCache();

  // Voice messages (Phase 4 A6) ---------------------------------------------

  void voiceRecordStart(int maxSeconds);

  Future<VoiceDraft> voiceRecordStop();

  void voiceRecordCancel();

  void voiceDraftDiscard(int draftId);

  Future<Message> sendVoiceMessage(
    String serverKey,
    int channelId,
    String nonce,
    int draftId,
  );

  Future<void> voicePlay(PlaybackItem item, int positionMs, double speed);

  /// Pause, resume, seek and speed act on whatever is playing.
  void voicePause();

  void voiceResume();

  void voiceSeek(int positionMs);

  void voiceSetSpeed(double speed);

  void voiceStop();

  Future<MediaHandle> mediaOpen(String serverKey, int attachmentId);

  void mediaPlay();

  void mediaPause();

  void mediaSeek(int positionMs);

  void mediaSetVolume(double volume, bool muted);

  void mediaClose();

  bool mediaSupported();

  // Private messages (Milestone E4, Task 17) --------------------------------

  /// Unavailable when the keyring could not be opened or initialized.
  RepoException? get privateUnavailable;

  /// Whether private messages opened fresh after moving a damaged database aside.
  bool get privateStartedFresh;

  void acknowledgePrivateStartedFresh();

  Future<List<PrivateContact>> privateContacts();

  Future<List<PrivateMessage>> privateMessages(
    String peer, {
    String? before,
    int limit = 50,
  });

  Future<PrivateMessage> sendPrivateMessage(String peer, String content);

  Future<void> editPrivateMessage(
    String peer,
    String messageId,
    String content,
  );

  Future<void> deletePrivateMessage(String peer, String messageId);

  Future<void> retryPrivateMessage(String messageId);

  Future<void> sendFriendRequest(String serverKey, int userId);

  Future<void> answerFriendRequest(String peer, {required bool accept});

  Future<void> cancelFriendRequest(String peer);

  Future<void> removeFriend(String peer);

  Future<void> acceptMessageRequest(String peer);

  Future<void> ignoreMessageRequest(String peer);

  Future<void> blockUser(String peer);

  Future<void> unblockUser(String peer);

  Future<PrivacySettings> privacySettings();

  Future<void> savePrivacySettings(PrivacySettings settings);

  Future<PrivateContact> openConversation(String serverKey, int userId);

  Future<void> moveConversation(String peer, String serverKey);

  Future<void> moveConversationToo(String peer, String messageId);

  Future<void> sendPrivateTyping(String peer);

  Future<void> markPrivateRead(String peer);

  Future<void> setPrivateMuted(String peer, {required bool muted});

  Future<void> hideConversation(String peer);
}

/// Overridden at startup with the mock or the Rust-core repository.
final repositoryProvider = Provider<OpencordRepository>(
  (ref) => throw StateError('repositoryProvider must be overridden'),
);
