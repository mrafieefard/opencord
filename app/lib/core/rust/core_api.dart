import 'dart:typed_data';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int64List;

import 'package:opencord/src/rust/api/attachments.dart' as frb_attachments;
import 'package:opencord/src/rust/api/client.dart' as frb;
import 'package:opencord/src/rust/api/types.dart' as core;

/// The Rust core's calls (its generated bindings), behind an interface so
/// the repository over them can be tested without the native library.
abstract interface class CoreApi {
  Stream<core.CoreEvent> eventStream();

  /// Voice media's connection states and device news.
  Stream<core.MediaEvent> mediaEventStream();

  Future<core.AudioDevices> audioDevices();

  void audioApplySettings(core.AudioSettings settings);

  void voiceSetPushToTalk(bool held);

  void voiceSetPrioritySpeaker(bool held);

  void audioSetLevelMeter(bool enabled);

  Future<void> audioMicTest(bool enabled);

  Future<core.NoiseSuppressionMode> audioRecommendedNoiseSuppression();

  Future<core.HotkeySupport> hotkeysSet(List<core.HotkeyBinding> bindings);

  /// 0–2 (200 %).
  void voiceSetUserVolume(String serverKey, int userId, double volume);

  void voiceSetUserLocalMute(String serverKey, int userId, bool muted);

  Future<void> voiceServerMute(String serverKey, int userId, bool muted);

  Future<void> voiceServerDeafen(String serverKey, int userId, bool deafened);

  Future<void> voiceMoveMember(String serverKey, int userId, int channelId);

  Future<void> voiceDisconnectMember(String serverKey, int userId);

  Future<core.ScreenAudioSupport> screenAudioSupport();

  Future<List<core.ScreenAudioApp>> screenAudioApps();

  void screenShareAudioFrom(String? app);

  /// 0–2 (200 %).
  void voiceSetStreamVolume(String serverKey, int userId, double volume);

  void voiceSetStreamMute(String serverKey, int userId, bool muted);

  /// What this system can do with video: receive, camera, screen, and
  /// whether the share dialog picks the source itself.
  core.VideoSupport videoSupport();

  /// Video draws into this engine's textures from now on.
  void videoInit(int engineHandle);

  Future<List<core.CameraDevice>> cameraDevices();

  Future<core.CameraStarted> cameraStart(String? deviceId);

  void cameraStop();

  Future<core.ScreenShareStarted> screenShareStart(
    core.ScreenShareRequest request,
    String? sourceId,
  );

  Future<List<core.ScreenSource>> screenSources();

  Future<void> screenShareUpdate(core.ScreenShareRequest request);

  Future<void> screenShareStop();

  Future<void> streamWatch(String streamKey);

  Future<void> streamUnwatch(String streamKey);

  void videoSetWants(List<core.VideoWant> wants);

  List<core.DefaultSound> soundboardDefaultSounds();

  Future<void> soundboardPlay(
    String serverKey,
    int channelId,
    String soundRef, {
    String? externalServerKey,
    core.SoundboardSound? externalSound,
  });

  Future<void> soundboardPreview(String serverKey, String soundRef);

  void soundboardSetListening(double volume, bool muted);
  void playCue(core.VoiceCue cue, double volume);
  bool audioAttenuationSupported();

  void soundboardMutePlayer(String serverKey, int userId, bool muted);

  Future<core.SoundboardSound> soundboardUpdateSound(
    String serverKey,
    int soundId,
    core.SoundboardSoundChanges changes,
  );

  Future<void> soundboardDeleteSound(String serverKey, int soundId);

  Future<core.SoundDraft> soundboardOpenDraft(String path);

  Future<void> soundboardPreviewDraft(core.SoundSelection selection);

  Future<core.SoundboardSound> soundboardUpload(
    String serverKey,
    core.SoundSelection selection,
    String name,
    String? emoji,
  );

  void soundboardCloseDraft(int draftId);

  core.GeneratedIdentity identityGenerate();

  /// Checks what [identityLoad] would use, without using it.
  core.IdentityInfo identityCheck(List<int> secret, String displayName);

  core.IdentityInfo identityLoad(List<int> secret, String displayName);

  String identityBackupEncode(List<int> secret);

  Uint8List identityBackupDecode(String backup);

  List<core.Server> serversList();

  Future<core.AddServerOutcome> serverAdd(
    String linkOrAddress,
    String? claimToken,
  );

  void serverTrustFingerprint(String address, String fingerprint);

  Future<void> serverRetryNow(String serverKey);

  Future<void> serverRemove(String serverKey);

  List<core.TrustedFingerprint> trustedFingerprints();

  Future<core.Message> sendMessage(
    String serverKey,
    int channelId,
    String content,
    String nonce,
  );

  Future<core.Message> editMessage(
    String serverKey,
    int messageId,
    String content,
  );

  Future<void> deleteMessage(String serverKey, int messageId);

  /// Newest first.
  Future<List<core.Message>> fetchMessages(
    String serverKey,
    int channelId,
    int? before,
    int limit,
  );

  Future<void> startTyping(String serverKey, int channelId);

  Future<core.Channel> createChannel(
    String serverKey,
    core.ChannelKind kind,
    String name,
    String? topic,
    int? parentId,
  );

  Future<core.Channel> updateChannel(
    String serverKey,
    int channelId,
    core.ChannelChanges changes,
  );

  Future<void> deleteChannel(String serverKey, int channelId);

  Future<void> reorderChannels(
    String serverKey,
    List<core.ChannelPosition> positions,
  );

  Future<core.Channel> setChannelOverwrite(
    String serverKey,
    int channelId,
    core.PermissionOverwrite overwrite,
  );

  Future<core.Channel> deleteChannelOverwrite(
    String serverKey,
    int channelId,
    core.OverwriteTargetKind kind,
    int targetId,
  );

  Future<core.Role> createRole(
    String serverKey, {
    required String name,
    required int color,
    required int permissions,
    required bool hoist,
    required bool mentionable,
    int? maxUploadBytes,
  });

  Future<core.Role> updateRole(
    String serverKey,
    int roleId,
    core.RoleChanges changes,
  );

  Future<void> deleteRole(String serverKey, int roleId);

  Future<void> reorderRoles(String serverKey, List<int> roleIds);

  Future<core.Member> addMemberRole(String serverKey, int userId, int roleId);

  Future<core.Member> removeMemberRole(
    String serverKey,
    int userId,
    int roleId,
  );

  Future<void> kickMember(String serverKey, int userId, String? reason);

  Future<void> banMember(String serverKey, int userId, String? reason);

  Future<void> unbanMember(String serverKey, int userId);

  Future<List<core.Ban>> fetchBans(String serverKey);

  Future<core.Member> updateNickname(
    String serverKey,
    int userId,
    String? nickname,
  );

  Future<core.User> updateProfile(String serverKey, String displayName);

  Future<void> updatePresence(String serverKey, core.PresenceStatus status);

  Future<core.Invite> createInvite(
    String serverKey, {
    int? maxUses,
    int? expiresInS,
    int? domainId,
  });

  Future<List<core.Invite>> fetchInvites(String serverKey);

  Future<void> revokeInvite(String serverKey, String code);

  Future<core.ServerInfo> updateServer(
    String serverKey,
    core.ServerChanges changes,
  );

  Future<core.VoiceSettings> updateVoiceSettings(
    String serverKey,
    core.VoiceSettingsChanges changes,
  );

  Future<core.NetworkInfo> watchNetwork(String serverKey, bool watch);

  Future<core.NetworkInfo> setPublicAddress(
    String serverKey, {
    required bool autoDetect,
    required String overrideIpv4,
    required String overrideIpv6,
  });

  Future<core.NetworkInfo> detectPublicAddress(String serverKey);

  Future<core.NetworkInfo> updateNetworkSettings(
    String serverKey,
    core.NetworkSettingsChanges changes,
  );

  Future<core.NetworkInfo> addDomain(
    String serverKey, {
    required String hostname,
    required core.DomainKind kind,
    int? publicPort,
  });

  Future<core.NetworkInfo> updateDomain(
    String serverKey,
    int domainId,
    core.DomainUpdate update,
  );

  Future<core.NetworkInfo> deleteDomain(String serverKey, int domainId);

  Future<core.NetworkInfo> checkDomain(String serverKey, int domainId);

  Future<List<core.DomainCheck>> domainChecks(String serverKey, int domainId);

  Future<core.NetworkInfo> requestCertificate(
    String serverKey,
    int domainId,
    String? email,
    bool agreeTerms,
  );

  Future<core.NetworkInfo> uploadCertificate(
    String serverKey,
    int domainId,
    String chainPem,
    String keyPem,
  );

  Future<core.NetworkInfo> setDnsProvider(
    String serverKey,
    String kind,
    String token,
  );

  Future<core.NetworkInfo> addFreeSubdomain(
    String serverKey,
    String provider,
    String name,
    String token,
    String? email,
    bool agreeTerms,
  );

  Future<core.NetworkInfo> startTunnel(
    String serverKey,
    core.TunnelKind kind,
    String token,
    String hostname,
  );

  Future<core.NetworkInfo> stopTunnel(String serverKey);

  Future<List<String>> tunnelLog(String serverKey);

  Future<core.NetworkInfo> installCloudflared(String serverKey);

  Future<core.LavalinkNodes> lavalinkNodes(
    String serverKey, {
    required bool watch,
  });

  Future<core.LavalinkNodes> createLavalinkNode(
    String serverKey,
    core.LavalinkNodeSettings settings,
    String password,
  );

  Future<core.LavalinkNodes> updateLavalinkNode(
    String serverKey,
    int nodeId,
    core.LavalinkNodeChanges changes,
  );

  Future<core.LavalinkNodes> deleteLavalinkNode(String serverKey, int nodeId);

  Future<void> testLavalinkNode(String serverKey, int nodeId);

  Future<core.LavalinkNodes> moveLavalinkPlayers(
    String serverKey,
    int fromNodeId,
    int? toNodeId,
  );

  Future<core.MusicSettings> musicSettings(String serverKey);

  Future<core.MusicSettings> updateMusicSettings(
    String serverKey,
    core.MusicSettingsChanges changes,
  );

  Future<List<core.MusicBlock>> musicBlocks(String serverKey);

  Future<List<core.MusicBlock>> blockFromMusic(
    String serverKey,
    int userId,
    bool blocked,
    String reason,
  );

  Future<core.MusicSearchResults> musicSearch(
    String serverKey,
    int channelId,
    String query,
    String source,
  );

  Future<core.MusicSession> musicSubscribe(String serverKey, int channelId);

  Future<void> musicUnsubscribe(String serverKey, int channelId);

  Future<core.MusicSession> musicEnqueue(
    String serverKey,
    int channelId,
    List<String> resultIds,
    List<String> historyItemIds,
    String? beforeItemId,
    core.MusicPlacement placement,
  );

  Future<core.MusicSession> musicPlayback(
    String serverKey,
    int channelId,
    core.MusicAction action, {
    int? positionMs,
    String? expectedItemId,
  });

  Future<core.MusicSession> musicSetMode(
    String serverKey,
    int channelId, {
    core.MusicRepeat? repeat,
    bool? shuffle,
  });

  Future<core.MusicSession> musicQueueMove(
    String serverKey,
    int channelId,
    String itemId,
    String? beforeItemId,
  );

  Future<core.MusicSession> musicQueueRemove(
    String serverKey,
    int channelId,
    List<String> itemIds,
  );

  Future<core.MusicSession> musicQueueClear(String serverKey, int channelId);

  Future<core.MusicSession> musicVoteSkip(
    String serverKey,
    int channelId,
    String expectedItemId,
  );

  /// 0–2 (200 %).
  void musicSetListening(double volume, bool muted, bool duckWhileTalking);

  /// The path of the artwork's file on this device.
  Future<String> musicArtwork(String serverKey, String artworkId, int size);

  Future<core.CommandOutcome> commandExecute(
    String serverKey,
    int channelId,
    String name,
    List<core.CommandOptionValue> options,
    String nonce,
  );

  Future<List<core.CommandChoice>> commandAutocomplete(
    String serverKey,
    int channelId,
    String name,
    String option,
    String value,
  );

  Future<core.CommandList> commandSettings(String serverKey);

  Future<core.CommandList> updateCommandSettings(
    String serverKey,
    core.CommandSettingsChanges changes,
  );

  /// Leaves any other voice channel first, on any server.
  Future<core.VoiceState> voiceJoin(String serverKey, int channelId);

  Future<void> voiceLeave();

  /// Holds outside voice too, for the next join.
  void voiceSetSelfMute(bool muted);

  void voiceSetSelfDeaf(bool deafened);

  Future<core.Message> sendMessageWithFiles({
    required String serverKey,
    required int channelId,
    required String content,
    required String nonce,
    required List<core.OutgoingFile> files,
  });

  void cancelSend({required String serverKey, required String nonce});

  Future<void> downloadAttachment({
    required String serverKey,
    required int attachmentId,
    required String path,
  });

  void cancelDownload({required String serverKey, required int attachmentId});

  Future<Uint8List> attachmentBytes({
    required String serverKey,
    required int attachmentId,
    required core.AttachmentVariant variant,
  });

  Future<Uint8List?> clipboardImage();

  Future<void> clipboardSetImage(Uint8List bytes);

  Future<core.UploadSettings> updateUploadSettings({
    required String serverKey,
    List<String>? blockedExtensions,
  });

  Future<core.StorageInfo> storageGet({required String serverKey});

  Future<core.StorageInfo> storageAdd({
    required String serverKey,
    required String name,
    required core.S3Config s3,
  });

  Future<core.StorageInfo> storageUpdate({
    required String serverKey,
    required int storageId,
    String? name,
    core.S3Config? s3,
  });

  Future<core.StorageTestResult> storageTest({
    required String serverKey,
    required core.StorageTestTarget target,
  });

  Future<core.StorageInfo> storageActivate({
    required String serverKey,
    required int storageId,
  });

  Future<core.StorageInfo> storageRemove({
    required String serverKey,
    required int storageId,
  });

  Future<core.StorageInfo> storageMoveStart({required String serverKey});

  Future<core.StorageInfo> storageMoveStop({required String serverKey});

  Future<BigInt> mediaCacheSize();

  Future<void> mediaCacheClear();

  void voiceRecordStart(int maxSeconds);

  core.VoiceDraft voiceRecordStop();

  void voiceRecordCancel();

  void voiceDraftDiscard(BigInt draftId);

  Future<core.Message> sendVoiceMessage(
    String serverKey,
    int channelId,
    String nonce,
    BigInt draftId,
  );

  Future<void> voicePlay(core.PlaybackItem item, int positionMs, double speed);

  void voicePause();

  void voiceResume();

  void voiceSeek(int positionMs);

  void voiceSetSpeed(double speed);

  void voiceStop();

  Future<core.MediaHandle> mediaOpen(String serverKey, int attachmentId);

  void mediaPlay();

  void mediaPause();

  void mediaSeek(int positionMs);

  void mediaSetVolume(double volume, bool muted);

  void mediaClose();

  bool mediaSupported();
}

/// [CoreApi] over the generated bindings; needs `RustLib.init` first.
class FrbCoreApi implements CoreApi {
  const FrbCoreApi();

  @override
  Stream<core.CoreEvent> eventStream() => frb.eventStream();

  @override
  Stream<core.MediaEvent> mediaEventStream() => frb.mediaEventStream();

  @override
  Future<core.AudioDevices> audioDevices() => frb.audioDevices();

  @override
  void audioApplySettings(core.AudioSettings settings) =>
      frb.audioApplySettings(settings: settings);

  @override
  void voiceSetPushToTalk(bool held) => frb.voiceSetPushToTalk(held: held);

  @override
  void voiceSetPrioritySpeaker(bool held) =>
      frb.voiceSetPrioritySpeaker(held: held);

  @override
  void audioSetLevelMeter(bool enabled) =>
      frb.audioSetLevelMeter(enabled: enabled);

  @override
  Future<void> audioMicTest(bool enabled) => frb.audioMicTest(enabled: enabled);

  @override
  Future<core.NoiseSuppressionMode> audioRecommendedNoiseSuppression() =>
      frb.audioRecommendedNoiseSuppression();

  @override
  Future<core.HotkeySupport> hotkeysSet(List<core.HotkeyBinding> bindings) =>
      frb.hotkeysSet(bindings: bindings);

  @override
  void voiceSetUserVolume(String serverKey, int userId, double volume) => frb
      .voiceSetUserVolume(serverKey: serverKey, userId: userId, volume: volume);

  @override
  Future<void> voiceServerMute(String serverKey, int userId, bool muted) =>
      frb.voiceServerMute(serverKey: serverKey, userId: userId, muted: muted);

  @override
  Future<void> voiceServerDeafen(String serverKey, int userId, bool deafened) =>
      frb.voiceServerDeafen(
        serverKey: serverKey,
        userId: userId,
        deafened: deafened,
      );

  @override
  Future<void> voiceMoveMember(String serverKey, int userId, int channelId) =>
      frb.voiceMoveMember(
        serverKey: serverKey,
        userId: userId,
        channelId: channelId,
      );

  @override
  Future<void> voiceDisconnectMember(String serverKey, int userId) =>
      frb.voiceDisconnectMember(serverKey: serverKey, userId: userId);

  @override
  Future<core.ScreenAudioSupport> screenAudioSupport() =>
      frb.screenAudioSupport();

  @override
  Future<List<core.ScreenAudioApp>> screenAudioApps() => frb.screenAudioApps();

  @override
  void screenShareAudioFrom(String? app) => frb.screenShareAudioFrom(app: app);

  @override
  void voiceSetStreamVolume(String serverKey, int userId, double volume) =>
      frb.voiceSetStreamVolume(
        serverKey: serverKey,
        userId: userId,
        volume: volume,
      );

  @override
  void voiceSetStreamMute(String serverKey, int userId, bool muted) => frb
      .voiceSetStreamMute(serverKey: serverKey, userId: userId, muted: muted);

  @override
  core.VideoSupport videoSupport() => frb.videoSupport();

  @override
  void videoInit(int engineHandle) => frb.videoInit(engineHandle: engineHandle);

  @override
  Future<List<core.CameraDevice>> cameraDevices() => frb.cameraDevices();

  @override
  Future<core.CameraStarted> cameraStart(String? deviceId) =>
      frb.cameraStart(deviceId: deviceId);

  @override
  void cameraStop() => frb.cameraStop();

  @override
  Future<core.ScreenShareStarted> screenShareStart(
    core.ScreenShareRequest request,
    String? sourceId,
  ) => frb.screenShareStart(request: request, sourceId: sourceId);

  @override
  Future<List<core.ScreenSource>> screenSources() => frb.screenSources();

  @override
  Future<void> screenShareUpdate(core.ScreenShareRequest request) =>
      frb.screenShareUpdate(request: request);

  @override
  Future<void> screenShareStop() => frb.screenShareStop();

  @override
  Future<void> streamWatch(String streamKey) =>
      frb.streamWatch(streamKey: streamKey);

  @override
  Future<void> streamUnwatch(String streamKey) =>
      frb.streamUnwatch(streamKey: streamKey);

  @override
  void videoSetWants(List<core.VideoWant> wants) =>
      frb.videoSetWants(wants: wants);

  @override
  List<core.DefaultSound> soundboardDefaultSounds() =>
      frb.soundboardDefaultSounds();

  @override
  Future<void> soundboardPlay(
    String serverKey,
    int channelId,
    String soundRef, {
    String? externalServerKey,
    core.SoundboardSound? externalSound,
  }) => frb.soundboardPlay(
    serverKey: serverKey,
    channelId: channelId,
    soundRef: soundRef,
    externalServerKey: externalServerKey,
    externalSound: externalSound,
  );

  @override
  Future<void> soundboardPreview(String serverKey, String soundRef) =>
      frb.soundboardPreview(serverKey: serverKey, soundRef: soundRef);

  @override
  void soundboardSetListening(double volume, bool muted) =>
      frb.soundboardSetListening(volume: volume, muted: muted);

  @override
  void playCue(core.VoiceCue cue, double volume) =>
      frb.playCue(cue: cue, volume: volume);

  @override
  bool audioAttenuationSupported() => frb.audioAttenuationSupported();

  @override
  void soundboardMutePlayer(String serverKey, int userId, bool muted) => frb
      .soundboardMutePlayer(serverKey: serverKey, userId: userId, muted: muted);

  @override
  Future<core.SoundboardSound> soundboardUpdateSound(
    String serverKey,
    int soundId,
    core.SoundboardSoundChanges changes,
  ) => frb.soundboardUpdateSound(
    serverKey: serverKey,
    soundId: soundId,
    changes: changes,
  );

  @override
  Future<void> soundboardDeleteSound(String serverKey, int soundId) =>
      frb.soundboardDeleteSound(serverKey: serverKey, soundId: soundId);

  @override
  Future<core.SoundDraft> soundboardOpenDraft(String path) =>
      frb.soundboardOpenDraft(path: path);

  @override
  Future<void> soundboardPreviewDraft(core.SoundSelection selection) =>
      frb.soundboardPreviewDraft(selection: selection);

  @override
  Future<core.SoundboardSound> soundboardUpload(
    String serverKey,
    core.SoundSelection selection,
    String name,
    String? emoji,
  ) => frb.soundboardUpload(
    serverKey: serverKey,
    selection: selection,
    name: name,
    emoji: emoji,
  );

  @override
  void soundboardCloseDraft(int draftId) =>
      frb.soundboardCloseDraft(draftId: BigInt.from(draftId));

  @override
  void voiceSetUserLocalMute(String serverKey, int userId, bool muted) =>
      frb.voiceSetUserLocalMute(
        serverKey: serverKey,
        userId: userId,
        muted: muted,
      );

  @override
  core.GeneratedIdentity identityGenerate() => frb.identityGenerate();

  @override
  core.IdentityInfo identityCheck(List<int> secret, String displayName) =>
      frb.identityCheck(secret: secret, displayName: displayName);

  @override
  core.IdentityInfo identityLoad(List<int> secret, String displayName) =>
      frb.identityLoad(secret: secret, displayName: displayName);

  @override
  String identityBackupEncode(List<int> secret) =>
      frb.identityBackupEncode(secret: secret);

  @override
  Uint8List identityBackupDecode(String backup) =>
      frb.identityBackupDecode(backup: backup);

  @override
  List<core.Server> serversList() => frb.serversList();

  @override
  Future<core.AddServerOutcome> serverAdd(
    String linkOrAddress,
    String? claimToken,
  ) => frb.serverAdd(linkOrAddress: linkOrAddress, claimToken: claimToken);

  @override
  void serverTrustFingerprint(String address, String fingerprint) =>
      frb.serverTrustFingerprint(address: address, fingerprint: fingerprint);

  @override
  Future<void> serverRetryNow(String serverKey) =>
      frb.serverRetryNow(serverKey: serverKey);

  @override
  Future<void> serverRemove(String serverKey) =>
      frb.serverRemove(serverKey: serverKey);

  @override
  List<core.TrustedFingerprint> trustedFingerprints() =>
      frb.trustedFingerprints();

  @override
  Future<core.Message> sendMessage(
    String serverKey,
    int channelId,
    String content,
    String nonce,
  ) => frb.sendMessage(
    serverKey: serverKey,
    channelId: channelId,
    content: content,
    nonce: nonce,
  );

  @override
  Future<core.Message> editMessage(
    String serverKey,
    int messageId,
    String content,
  ) => frb.editMessage(
    serverKey: serverKey,
    messageId: messageId,
    content: content,
  );

  @override
  Future<void> deleteMessage(String serverKey, int messageId) =>
      frb.deleteMessage(serverKey: serverKey, messageId: messageId);

  @override
  Future<List<core.Message>> fetchMessages(
    String serverKey,
    int channelId,
    int? before,
    int limit,
  ) => frb.fetchMessages(
    serverKey: serverKey,
    channelId: channelId,
    before: before,
    limit: limit,
  );

  @override
  Future<void> startTyping(String serverKey, int channelId) =>
      frb.startTyping(serverKey: serverKey, channelId: channelId);

  @override
  Future<core.Channel> createChannel(
    String serverKey,
    core.ChannelKind kind,
    String name,
    String? topic,
    int? parentId,
  ) => frb.createChannel(
    serverKey: serverKey,
    kind: kind,
    name: name,
    topic: topic,
    parentId: parentId,
  );

  @override
  Future<core.Channel> updateChannel(
    String serverKey,
    int channelId,
    core.ChannelChanges changes,
  ) => frb.updateChannel(
    serverKey: serverKey,
    channelId: channelId,
    changes: changes,
  );

  @override
  Future<void> deleteChannel(String serverKey, int channelId) =>
      frb.deleteChannel(serverKey: serverKey, channelId: channelId);

  @override
  Future<void> reorderChannels(
    String serverKey,
    List<core.ChannelPosition> positions,
  ) => frb.reorderChannels(serverKey: serverKey, positions: positions);

  @override
  Future<core.Channel> setChannelOverwrite(
    String serverKey,
    int channelId,
    core.PermissionOverwrite overwrite,
  ) => frb.setChannelOverwrite(
    serverKey: serverKey,
    channelId: channelId,
    overwrite: overwrite,
  );

  @override
  Future<core.Channel> deleteChannelOverwrite(
    String serverKey,
    int channelId,
    core.OverwriteTargetKind kind,
    int targetId,
  ) => frb.deleteChannelOverwrite(
    serverKey: serverKey,
    channelId: channelId,
    targetKind: kind,
    targetId: targetId,
  );

  @override
  Future<core.Role> createRole(
    String serverKey, {
    required String name,
    required int color,
    required int permissions,
    required bool hoist,
    required bool mentionable,
    int? maxUploadBytes,
  }) => frb.createRole(
    serverKey: serverKey,
    name: name,
    color: color,
    permissions: permissions,
    hoist: hoist,
    mentionable: mentionable,
    maxUploadBytes: maxUploadBytes != null ? BigInt.from(maxUploadBytes) : null,
  );

  @override
  Future<core.Role> updateRole(
    String serverKey,
    int roleId,
    core.RoleChanges changes,
  ) => frb.updateRole(serverKey: serverKey, roleId: roleId, changes: changes);

  @override
  Future<void> deleteRole(String serverKey, int roleId) =>
      frb.deleteRole(serverKey: serverKey, roleId: roleId);

  @override
  Future<void> reorderRoles(String serverKey, List<int> roleIds) => frb
      .reorderRoles(serverKey: serverKey, roleIds: Int64List.fromList(roleIds));

  @override
  Future<core.Member> addMemberRole(String serverKey, int userId, int roleId) =>
      frb.addMemberRole(serverKey: serverKey, userId: userId, roleId: roleId);

  @override
  Future<core.Member> removeMemberRole(
    String serverKey,
    int userId,
    int roleId,
  ) => frb.removeMemberRole(
    serverKey: serverKey,
    userId: userId,
    roleId: roleId,
  );

  @override
  Future<void> kickMember(String serverKey, int userId, String? reason) =>
      frb.kickMember(serverKey: serverKey, userId: userId, reason: reason);

  @override
  Future<void> banMember(String serverKey, int userId, String? reason) =>
      frb.banMember(serverKey: serverKey, userId: userId, reason: reason);

  @override
  Future<void> unbanMember(String serverKey, int userId) =>
      frb.unbanMember(serverKey: serverKey, userId: userId);

  @override
  Future<List<core.Ban>> fetchBans(String serverKey) =>
      frb.fetchBans(serverKey: serverKey);

  @override
  Future<core.Member> updateNickname(
    String serverKey,
    int userId,
    String? nickname,
  ) => frb.updateNickname(
    serverKey: serverKey,
    userId: userId,
    nickname: nickname,
  );

  @override
  Future<core.User> updateProfile(String serverKey, String displayName) =>
      frb.updateProfile(serverKey: serverKey, displayName: displayName);

  @override
  Future<void> updatePresence(String serverKey, core.PresenceStatus status) =>
      frb.updatePresence(serverKey: serverKey, status: status);

  @override
  Future<core.Invite> createInvite(
    String serverKey, {
    int? maxUses,
    int? expiresInS,
    int? domainId,
  }) => frb.createInvite(
    serverKey: serverKey,
    maxUses: maxUses,
    expiresInS: expiresInS,
    domainId: domainId,
  );

  @override
  Future<List<core.Invite>> fetchInvites(String serverKey) =>
      frb.fetchInvites(serverKey: serverKey);

  @override
  Future<void> revokeInvite(String serverKey, String code) =>
      frb.revokeInvite(serverKey: serverKey, code: code);

  @override
  Future<core.ServerInfo> updateServer(
    String serverKey,
    core.ServerChanges changes,
  ) => frb.updateServer(serverKey: serverKey, changes: changes);

  @override
  Future<core.VoiceSettings> updateVoiceSettings(
    String serverKey,
    core.VoiceSettingsChanges changes,
  ) => frb.updateVoiceSettings(serverKey: serverKey, changes: changes);

  @override
  Future<core.NetworkInfo> watchNetwork(String serverKey, bool watch) =>
      frb.watchNetwork(serverKey: serverKey, watch: watch);

  @override
  Future<core.NetworkInfo> setPublicAddress(
    String serverKey, {
    required bool autoDetect,
    required String overrideIpv4,
    required String overrideIpv6,
  }) => frb.setPublicAddress(
    serverKey: serverKey,
    autoDetect: autoDetect,
    overrideIpv4: overrideIpv4,
    overrideIpv6: overrideIpv6,
  );

  @override
  Future<core.NetworkInfo> detectPublicAddress(String serverKey) =>
      frb.detectPublicAddress(serverKey: serverKey);

  @override
  Future<core.NetworkInfo> updateNetworkSettings(
    String serverKey,
    core.NetworkSettingsChanges changes,
  ) => frb.updateNetworkSettings(serverKey: serverKey, changes: changes);

  @override
  Future<core.NetworkInfo> addDomain(
    String serverKey, {
    required String hostname,
    required core.DomainKind kind,
    int? publicPort,
  }) => frb.addDomain(
    serverKey: serverKey,
    hostname: hostname,
    kind: kind,
    publicPort: publicPort,
  );

  @override
  Future<core.NetworkInfo> updateDomain(
    String serverKey,
    int domainId,
    core.DomainUpdate update,
  ) => frb.updateDomain(
    serverKey: serverKey,
    domainId: domainId,
    update: update,
  );

  @override
  Future<core.NetworkInfo> deleteDomain(String serverKey, int domainId) =>
      frb.deleteDomain(serverKey: serverKey, domainId: domainId);

  @override
  Future<core.NetworkInfo> checkDomain(String serverKey, int domainId) =>
      frb.checkDomain(serverKey: serverKey, domainId: domainId);

  @override
  Future<List<core.DomainCheck>> domainChecks(String serverKey, int domainId) =>
      frb.domainChecks(serverKey: serverKey, domainId: domainId);

  @override
  Future<core.NetworkInfo> requestCertificate(
    String serverKey,
    int domainId,
    String? email,
    bool agreeTerms,
  ) => frb.requestCertificate(
    serverKey: serverKey,
    domainId: domainId,
    email: email,
    agreeTerms: agreeTerms,
  );

  @override
  Future<core.NetworkInfo> uploadCertificate(
    String serverKey,
    int domainId,
    String chainPem,
    String keyPem,
  ) => frb.uploadCertificate(
    serverKey: serverKey,
    domainId: domainId,
    chainPem: chainPem,
    keyPem: keyPem,
  );

  @override
  Future<core.NetworkInfo> setDnsProvider(
    String serverKey,
    String kind,
    String token,
  ) => frb.setDnsProvider(serverKey: serverKey, kind: kind, token: token);

  @override
  Future<core.NetworkInfo> addFreeSubdomain(
    String serverKey,
    String provider,
    String name,
    String token,
    String? email,
    bool agreeTerms,
  ) => frb.addFreeSubdomain(
    serverKey: serverKey,
    provider: provider,
    name: name,
    token: token,
    email: email,
    agreeTerms: agreeTerms,
  );

  @override
  Future<core.NetworkInfo> startTunnel(
    String serverKey,
    core.TunnelKind kind,
    String token,
    String hostname,
  ) => frb.startTunnel(
    serverKey: serverKey,
    kind: kind,
    token: token,
    hostname: hostname,
  );

  @override
  Future<core.NetworkInfo> stopTunnel(String serverKey) =>
      frb.stopTunnel(serverKey: serverKey);

  @override
  Future<List<String>> tunnelLog(String serverKey) =>
      frb.tunnelLog(serverKey: serverKey);

  @override
  Future<core.NetworkInfo> installCloudflared(String serverKey) =>
      frb.installCloudflared(serverKey: serverKey);

  @override
  Future<core.LavalinkNodes> lavalinkNodes(
    String serverKey, {
    required bool watch,
  }) => frb.lavalinkNodes(serverKey: serverKey, watch: watch);

  @override
  Future<core.LavalinkNodes> createLavalinkNode(
    String serverKey,
    core.LavalinkNodeSettings settings,
    String password,
  ) => frb.createLavalinkNode(
    serverKey: serverKey,
    settings: settings,
    password: password,
  );

  @override
  Future<core.LavalinkNodes> updateLavalinkNode(
    String serverKey,
    int nodeId,
    core.LavalinkNodeChanges changes,
  ) => frb.updateLavalinkNode(
    serverKey: serverKey,
    nodeId: nodeId,
    changes: changes,
  );

  @override
  Future<core.LavalinkNodes> deleteLavalinkNode(String serverKey, int nodeId) =>
      frb.deleteLavalinkNode(serverKey: serverKey, nodeId: nodeId);

  @override
  Future<void> testLavalinkNode(String serverKey, int nodeId) =>
      frb.testLavalinkNode(serverKey: serverKey, nodeId: nodeId);

  @override
  Future<core.LavalinkNodes> moveLavalinkPlayers(
    String serverKey,
    int fromNodeId,
    int? toNodeId,
  ) => frb.moveLavalinkPlayers(
    serverKey: serverKey,
    fromNodeId: fromNodeId,
    toNodeId: toNodeId,
  );

  @override
  Future<core.MusicSettings> musicSettings(String serverKey) =>
      frb.musicSettings(serverKey: serverKey);

  @override
  Future<core.MusicSettings> updateMusicSettings(
    String serverKey,
    core.MusicSettingsChanges changes,
  ) => frb.updateMusicSettings(serverKey: serverKey, changes: changes);

  @override
  Future<List<core.MusicBlock>> musicBlocks(String serverKey) =>
      frb.musicBlocks(serverKey: serverKey);

  @override
  Future<List<core.MusicBlock>> blockFromMusic(
    String serverKey,
    int userId,
    bool blocked,
    String reason,
  ) => frb.blockFromMusic(
    serverKey: serverKey,
    userId: userId,
    blocked: blocked,
    reason: reason,
  );

  @override
  Future<core.MusicSearchResults> musicSearch(
    String serverKey,
    int channelId,
    String query,
    String source,
  ) => frb.musicSearch(
    serverKey: serverKey,
    channelId: channelId,
    query: query,
    source: source,
  );

  @override
  Future<core.MusicSession> musicSubscribe(String serverKey, int channelId) =>
      frb.musicSubscribe(serverKey: serverKey, channelId: channelId);

  @override
  Future<void> musicUnsubscribe(String serverKey, int channelId) =>
      frb.musicUnsubscribe(serverKey: serverKey, channelId: channelId);

  @override
  Future<core.MusicSession> musicEnqueue(
    String serverKey,
    int channelId,
    List<String> resultIds,
    List<String> historyItemIds,
    String? beforeItemId,
    core.MusicPlacement placement,
  ) => frb.musicEnqueue(
    serverKey: serverKey,
    channelId: channelId,
    resultIds: resultIds,
    historyItemIds: historyItemIds,
    beforeItemId: beforeItemId,
    placement: placement,
  );

  @override
  Future<core.MusicSession> musicPlayback(
    String serverKey,
    int channelId,
    core.MusicAction action, {
    int? positionMs,
    String? expectedItemId,
  }) => frb.musicPlayback(
    serverKey: serverKey,
    channelId: channelId,
    action: action,
    positionMs: positionMs == null ? null : BigInt.from(positionMs),
    expectedItemId: expectedItemId,
  );

  @override
  Future<core.MusicSession> musicSetMode(
    String serverKey,
    int channelId, {
    core.MusicRepeat? repeat,
    bool? shuffle,
  }) => frb.musicSetMode(
    serverKey: serverKey,
    channelId: channelId,
    repeat: repeat,
    shuffle: shuffle,
  );

  @override
  Future<core.MusicSession> musicQueueMove(
    String serverKey,
    int channelId,
    String itemId,
    String? beforeItemId,
  ) => frb.musicQueueMove(
    serverKey: serverKey,
    channelId: channelId,
    itemId: itemId,
    beforeItemId: beforeItemId,
  );

  @override
  Future<core.MusicSession> musicQueueRemove(
    String serverKey,
    int channelId,
    List<String> itemIds,
  ) => frb.musicQueueRemove(
    serverKey: serverKey,
    channelId: channelId,
    itemIds: itemIds,
  );

  @override
  Future<core.MusicSession> musicQueueClear(String serverKey, int channelId) =>
      frb.musicQueueClear(serverKey: serverKey, channelId: channelId);

  @override
  Future<core.MusicSession> musicVoteSkip(
    String serverKey,
    int channelId,
    String expectedItemId,
  ) => frb.musicVoteSkip(
    serverKey: serverKey,
    channelId: channelId,
    expectedItemId: expectedItemId,
  );

  @override
  void musicSetListening(double volume, bool muted, bool duckWhileTalking) =>
      frb.musicSetListening(
        volume: volume,
        muted: muted,
        duckWhileTalking: duckWhileTalking,
      );

  @override
  Future<String> musicArtwork(String serverKey, String artworkId, int size) =>
      frb.musicArtwork(serverKey: serverKey, artworkId: artworkId, size: size);

  @override
  Future<core.CommandOutcome> commandExecute(
    String serverKey,
    int channelId,
    String name,
    List<core.CommandOptionValue> options,
    String nonce,
  ) => frb.commandExecute(
    serverKey: serverKey,
    channelId: channelId,
    name: name,
    options: options,
    nonce: nonce,
  );

  @override
  Future<List<core.CommandChoice>> commandAutocomplete(
    String serverKey,
    int channelId,
    String name,
    String option,
    String value,
  ) => frb.commandAutocomplete(
    serverKey: serverKey,
    channelId: channelId,
    name: name,
    option: option,
    value: value,
  );

  @override
  Future<core.CommandList> commandSettings(String serverKey) =>
      frb.commandSettings(serverKey: serverKey);

  @override
  Future<core.CommandList> updateCommandSettings(
    String serverKey,
    core.CommandSettingsChanges changes,
  ) => frb.updateCommandSettings(serverKey: serverKey, changes: changes);

  @override
  Future<core.VoiceState> voiceJoin(String serverKey, int channelId) =>
      frb.voiceJoin(serverKey: serverKey, channelId: channelId);

  @override
  Future<void> voiceLeave() => frb.voiceLeave();

  @override
  void voiceSetSelfMute(bool muted) => frb.voiceSetSelfMute(muted: muted);

  @override
  void voiceSetSelfDeaf(bool deafened) =>
      frb.voiceSetSelfDeaf(deafened: deafened);

  @override
  Future<core.Message> sendMessageWithFiles({
    required String serverKey,
    required int channelId,
    required String content,
    required String nonce,
    required List<core.OutgoingFile> files,
  }) => frb_attachments.sendMessageWithFiles(
    serverKey: serverKey,
    channelId: channelId,
    content: content,
    nonce: nonce,
    files: files,
  );

  @override
  void cancelSend({required String serverKey, required String nonce}) =>
      frb_attachments.cancelSend(serverKey: serverKey, nonce: nonce);

  @override
  Future<void> downloadAttachment({
    required String serverKey,
    required int attachmentId,
    required String path,
  }) => frb_attachments.downloadAttachment(
    serverKey: serverKey,
    attachmentId: attachmentId,
    path: path,
  );

  @override
  void cancelDownload({required String serverKey, required int attachmentId}) =>
      frb_attachments.cancelDownload(
        serverKey: serverKey,
        attachmentId: attachmentId,
      );

  @override
  Future<Uint8List> attachmentBytes({
    required String serverKey,
    required int attachmentId,
    required core.AttachmentVariant variant,
  }) => frb_attachments.attachmentBytes(
    serverKey: serverKey,
    attachmentId: attachmentId,
    variant: variant,
  );

  @override
  Future<Uint8List?> clipboardImage() => frb_attachments.clipboardImage();

  @override
  Future<void> clipboardSetImage(Uint8List bytes) =>
      frb_attachments.clipboardSetImage(bytes: bytes);

  @override
  Future<core.UploadSettings> updateUploadSettings({
    required String serverKey,
    List<String>? blockedExtensions,
  }) => frb_attachments.updateUploadSettings(
    serverKey: serverKey,
    blockedExtensions: blockedExtensions,
  );

  @override
  Future<core.StorageInfo> storageGet({required String serverKey}) =>
      frb_attachments.storageGet(serverKey: serverKey);

  @override
  Future<core.StorageInfo> storageAdd({
    required String serverKey,
    required String name,
    required core.S3Config s3,
  }) => frb_attachments.storageAdd(serverKey: serverKey, name: name, s3: s3);

  @override
  Future<core.StorageInfo> storageUpdate({
    required String serverKey,
    required int storageId,
    String? name,
    core.S3Config? s3,
  }) => frb_attachments.storageUpdate(
    serverKey: serverKey,
    storageId: storageId,
    name: name,
    s3: s3,
  );

  @override
  Future<core.StorageTestResult> storageTest({
    required String serverKey,
    required core.StorageTestTarget target,
  }) => frb_attachments.storageTest(serverKey: serverKey, target: target);

  @override
  Future<core.StorageInfo> storageActivate({
    required String serverKey,
    required int storageId,
  }) => frb_attachments.storageActivate(
    serverKey: serverKey,
    storageId: storageId,
  );

  @override
  Future<core.StorageInfo> storageRemove({
    required String serverKey,
    required int storageId,
  }) =>
      frb_attachments.storageRemove(serverKey: serverKey, storageId: storageId);

  @override
  Future<core.StorageInfo> storageMoveStart({required String serverKey}) =>
      frb_attachments.storageMoveStart(serverKey: serverKey);

  @override
  Future<core.StorageInfo> storageMoveStop({required String serverKey}) =>
      frb_attachments.storageMoveStop(serverKey: serverKey);

  @override
  Future<BigInt> mediaCacheSize() => frb_attachments.mediaCacheSize();

  @override
  Future<void> mediaCacheClear() => frb_attachments.mediaCacheClear();

  @override
  void voiceRecordStart(int maxSeconds) =>
      frb_attachments.voiceRecordStart(maxSeconds: maxSeconds);

  @override
  core.VoiceDraft voiceRecordStop() => frb_attachments.voiceRecordStop();

  @override
  void voiceRecordCancel() => frb_attachments.voiceRecordCancel();

  @override
  void voiceDraftDiscard(BigInt draftId) =>
      frb_attachments.voiceDraftDiscard(draftId: draftId);

  @override
  Future<core.Message> sendVoiceMessage(
    String serverKey,
    int channelId,
    String nonce,
    BigInt draftId,
  ) => frb_attachments.sendVoiceMessage(
    serverKey: serverKey,
    channelId: channelId,
    nonce: nonce,
    draftId: draftId,
  );

  @override
  Future<void> voicePlay(
    core.PlaybackItem item,
    int positionMs,
    double speed,
  ) => frb_attachments.voicePlay(
    item: item,
    positionMs: positionMs,
    speed: speed,
  );

  @override
  void voicePause() => frb_attachments.voicePause();

  @override
  void voiceResume() => frb_attachments.voiceResume();

  @override
  void voiceSeek(int positionMs) =>
      frb_attachments.voiceSeek(positionMs: positionMs);

  @override
  void voiceSetSpeed(double speed) =>
      frb_attachments.voiceSetSpeed(speed: speed);

  @override
  void voiceStop() => frb_attachments.voiceStop();

  @override
  Future<core.MediaHandle> mediaOpen(String serverKey, int attachmentId) =>
      frb_attachments.mediaOpen(
        serverKey: serverKey,
        attachmentId: attachmentId,
      );

  @override
  void mediaPlay() => frb_attachments.mediaPlay();

  @override
  void mediaPause() => frb_attachments.mediaPause();

  @override
  void mediaSeek(int positionMs) =>
      frb_attachments.mediaSeek(positionMs: positionMs);

  @override
  void mediaSetVolume(double volume, bool muted) =>
      frb_attachments.mediaSetVolume(volume: volume, muted: muted);

  @override
  void mediaClose() => frb_attachments.mediaClose();

  @override
  bool mediaSupported() => frb_attachments.mediaSupported();
}
