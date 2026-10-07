import 'dart:typed_data';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int64List;

import 'package:opencord/src/rust/api/client.dart' as frb;
import 'package:opencord/src/rust/api/types.dart' as core;

/// The Rust core's calls (its generated bindings), behind an interface so
/// the repository over them can be tested without the native library.
abstract interface class CoreApi {
  Stream<core.CoreEvent> eventStream();

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

  /// Leaves any other voice channel first, on any server.
  Future<core.VoiceState> voiceJoin(String serverKey, int channelId);

  Future<void> voiceLeave();

  /// Holds outside voice too, for the next join.
  void voiceSetSelfMute(bool muted);

  void voiceSetSelfDeaf(bool deafened);
}

/// [CoreApi] over the generated bindings; needs `RustLib.init` first.
class FrbCoreApi implements CoreApi {
  const FrbCoreApi();

  @override
  Stream<core.CoreEvent> eventStream() => frb.eventStream();

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
  }) => frb.createRole(
    serverKey: serverKey,
    name: name,
    color: color,
    permissions: permissions,
    hoist: hoist,
    mentionable: mentionable,
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
  }) => frb.createInvite(
    serverKey: serverKey,
    maxUses: maxUses,
    expiresInS: expiresInS,
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
  Future<core.VoiceState> voiceJoin(String serverKey, int channelId) =>
      frb.voiceJoin(serverKey: serverKey, channelId: channelId);

  @override
  Future<void> voiceLeave() => frb.voiceLeave();

  @override
  void voiceSetSelfMute(bool muted) => frb.voiceSetSelfMute(muted: muted);

  @override
  void voiceSetSelfDeaf(bool deafened) =>
      frb.voiceSetSelfDeaf(deafened: deafened);
}
