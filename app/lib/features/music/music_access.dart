import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// What this member may do with a channel's music (Phase 3 plan §16.1).
/// The server checks all of it again; this only hides and disables.
@immutable
class MusicAccess {
  const MusicAccess({
    this.connected = false,
    this.permissions = Permissions.none,
    this.selfId = 0,
    this.manageServer = false,
  });

  /// In the channel's voice on this device.
  final bool connected;
  final Permissions permissions;
  final int selfId;
  final bool manageServer;

  bool get manage => permissions.has(Permissions.musicManage);

  /// Connected, or managing music from outside.
  bool get present => connected || manage;

  bool get canAdd => present && permissions.has(Permissions.musicAdd);

  bool get canControl => present && permissions.has(Permissions.musicControl);

  bool get canManageQueue =>
      present && permissions.has(Permissions.musicManageQueue);

  /// Remove or skip one's own track with Add music; others' need more.
  bool canRemove(int requesterId) =>
      canManageQueue || (canAdd && requesterId == selfId);

  bool canSkip(int? requesterId) =>
      canControl || (canAdd && requesterId == selfId);

  /// Why a control is off, for its tooltip.
  String? why(Permissions needed) {
    if (!present) return 'Join the voice channel to control music';
    if (!permissions.has(needed)) return 'Needs ${permissionName(needed)}';
    return null;
  }

  @override
  bool operator ==(Object other) =>
      other is MusicAccess &&
      other.connected == connected &&
      other.permissions == permissions &&
      other.selfId == selfId &&
      other.manageServer == manageServer;

  @override
  int get hashCode => Object.hash(connected, permissions, selfId, manageServer);
}

String permissionName(Permissions permission) =>
    permission == Permissions.musicAdd
    ? 'Add music'
    : permission == Permissions.musicControl
    ? 'Control playback'
    : permission == Permissions.musicManageQueue
    ? 'Manage the queue'
    : 'Manage music';

final musicAccessProvider = Provider.autoDispose
    .family<MusicAccess, MusicChannel>((ref, channel) {
      final voice = ref.watch(voiceSessionProvider);
      final (permissions, selfId, manageServer) = ref.watch(
        serverProvider(channel.server).select(
          (state) => (
            state.data?.permissionsIn(channel.channel) ?? Permissions.none,
            state.data?.self.id ?? 0,
            state.data?.can(Permissions.manageServer) ?? false,
          ),
        ),
      );
      return MusicAccess(
        connected:
            voice.serverKey == channel.server &&
            voice.channelId == channel.channel,
        permissions: permissions,
        selfId: selfId,
        manageServer: manageServer,
      );
    });

/// The voice channel whose music the right-side panel shows (§19.3): the
/// one open, else the one this device is connected to.
final musicChannelProvider = Provider<MusicChannel?>((ref) {
  final server = ref.watch(currentServerProvider);
  final channelId = ref.watch(currentChannelProvider);
  if (server != null && channelId != null) {
    final kind = ref.watch(
      serverProvider(
        server,
      ).select((state) => state.data?.channels[channelId]?.kind),
    );
    if (kind == ChannelKind.voice) return (server: server, channel: channelId);
  }
  final voice = ref.watch(voiceSessionProvider);
  final (voiceServer, voiceChannel) = (voice.serverKey, voice.channelId);
  if (voiceServer == null || voiceChannel == null) return null;
  return (server: voiceServer, channel: voiceChannel);
});

/// Runs a music action; what the server refuses shows as a toast, and an
/// answer replaces the session at once.
Future<void> musicAction(
  BuildContext context,
  WidgetRef ref,
  MusicChannel channel,
  Future<MusicSession?> Function(OpencordRepository repository) action,
) async {
  // The row or button may be gone by the answer; the providers are not.
  final container = ProviderScope.containerOf(context, listen: false);
  try {
    final session = await action(container.read(repositoryProvider));
    final provider = musicSessionProvider(channel);
    if (session != null && container.exists(provider)) {
      container.read(provider.notifier).apply(session);
    }
  } on RepoException catch (error) {
    // A track that changed meanwhile: the new state is on its way.
    if (error.kind == RepoErrorKind.musicStale) return;
    if (context.mounted) showOcToast(context, error.message);
  }
}
