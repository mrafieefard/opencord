import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/voice/camera_menu.dart';
import 'package:opencord/features/voice/screenshare_picker.dart';
import 'package:opencord/features/voice/sound_from.dart';
import 'package:opencord/features/voice/voice_dock.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

export 'voice_dock.dart';

/// Joins a voice channel; why not, when the server refuses, shows as a
/// toast.
Future<void> joinVoice(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  int channelId,
) => _joinOrSay(
  ref.read(voiceSessionProvider.notifier),
  ocToaster(context),
  serverKey,
  channelId,
);

Future<void> _joinOrSay(
  VoiceSessionNotifier session,
  OcToaster toast,
  String serverKey,
  int channelId,
) async {
  try {
    await session.join(serverKey, channelId);
  } on RepoException catch (error) {
    toast(switch (error.kind) {
      RepoErrorKind.voiceChannelFull => 'This voice channel is full.',
      RepoErrorKind.forbidden =>
        "You don't have permission to join this channel.",
      _ => error.message,
    });
  }
}

/// Turns the camera on or off; why it could not start shows as a toast.
Future<void> toggleCamera(BuildContext context, WidgetRef ref) async {
  try {
    await ref.read(voiceSessionProvider.notifier).toggleCamera();
  } on RepoException catch (error) {
    if (!context.mounted) return;
    showOcToast(context, cameraErrorText(error));
  }
}

/// Goes live (§9.1); while live, offers a new source or quality, or
/// stopping. [anchor] is where the menu opens (a global rectangle).
Future<void> screenshareControl(
  BuildContext context,
  WidgetRef ref, {
  Rect? anchor,
}) async {
  final voice = ref.read(voiceSessionProvider);
  final server = voice.serverKey;
  if (server == null) return;
  if (!voice.screensharing) return goLive(context, ref, server);
  final share = ref.read(ownScreenShareProvider);
  final sharing = ref.read(ownScreenShareProvider.notifier);
  await showOcMenu(
    context: context,
    position: (anchor ?? Rect.zero).topCenter,
    entries: [
      OcMenuItem(
        label: 'Change source',
        icon: OcIcons.monitor,
        onSelected: share == null
            ? null
            : () async {
                final navigator = Navigator.of(context);
                // Where Opencord picks, "Change source" has to ask which
                // one; where the desktop picks, starting again is what
                // opens its picker.
                String? source;
                if (ref.read(repositoryProvider).capabilities.ownScreenPicker) {
                  final choice = await showScreenshareDialog(
                    context,
                    serverKey: server,
                    current: share.quality,
                    pickingSource: true,
                  );
                  if (choice == null || !navigator.mounted) return;
                  source = choice.sourceId;
                }
                await _sharing(
                  navigator.context,
                  () => sharing.start(
                    server,
                    share.quality,
                    audio: share.audio,
                    sourceId: source,
                  ),
                );
                await _askSoundFrom(navigator);
              },
      ),
      OcMenuItem(
        label: 'Change quality',
        icon: OcIcons.tune,
        onSelected: share == null
            ? null
            : () => _changeQuality(context, ref, server, share.quality),
      ),
      if (ref.read(screenAudioSupportProvider).value?.supported ?? false)
        OcMenuItem(
          label: 'Share sound',
          icon: OcIcons.volumeUp,
          checked: share?.audio ?? false,
          onSelected: share == null
              ? null
              : () async {
                  final navigator = Navigator.of(context);
                  await _sharing(
                    context,
                    () => sharing.changeQuality(
                      server,
                      share.quality,
                      audio: !share.audio,
                    ),
                  );
                  if (!share.audio) await _askSoundFrom(navigator);
                },
        ),
      if (share != null && share.audio && share.source == StreamSource.window)
        OcMenuItem(
          label: share.audioFrom == null
              ? 'Sound from…'
              : 'Sound from ${share.audioFrom}…',
          icon: OcIcons.musicNote,
          onSelected: () => chooseSoundFrom(context),
        ),
      const OcMenuDivider(),
      OcMenuItem(
        label: 'Stop sharing',
        icon: OcIcons.stopScreenShare,
        onSelected: () => _sharing(context, sharing.stop),
      ),
    ],
  );
}

/// The share dialog — which on a system with Opencord's own picker also
/// chooses the screen or window — then the system's picker where there is
/// one instead.
Future<void> goLive(BuildContext context, WidgetRef ref, String server) async {
  // The button asking goes once live: dialogs after it open from here.
  final navigator = Navigator.of(context);
  final choice = await showScreenshareDialog(context, serverKey: server);
  if (choice == null || !context.mounted) return;
  // The call may have ended, or sharing begun elsewhere, meanwhile.
  final voice = ref.read(voiceSessionProvider);
  if (!voice.connected || voice.screensharing) return;
  await _sharing(
    context,
    () => ref
        .read(ownScreenShareProvider.notifier)
        .start(
          server,
          choice.quality,
          audio: choice.audio,
          sourceId: choice.sourceId,
        ),
  );
  await _askSoundFrom(navigator);
}

/// A window shared with its sound asks which app it is (§10.4).
Future<void> _askSoundFrom(NavigatorState navigator) async {
  if (!navigator.mounted) return;
  final container = ProviderScope.containerOf(navigator.context, listen: false);
  // The repository's news of the share arrives a moment after.
  await Future<void>.delayed(Duration.zero);
  final share = container.read(ownScreenShareProvider);
  // Where the system says which app the window is, its sound already goes.
  if (share == null ||
      !share.audio ||
      share.source != StreamSource.window ||
      share.audioFrom != null) {
    return;
  }
  if (navigator.mounted) await chooseSoundFrom(navigator.context);
}

Future<void> _changeQuality(
  BuildContext context,
  WidgetRef ref,
  String server,
  ScreenShareQuality current,
) async {
  final choice = await showScreenshareDialog(
    context,
    serverKey: server,
    current: current,
    changing: true,
  );
  if (choice == null || !context.mounted) return;
  await _sharing(
    context,
    () => ref
        .read(ownScreenShareProvider.notifier)
        .changeQuality(server, choice.quality, audio: choice.audio),
  );
}

/// Runs a screen share action; why it failed shows as a toast. Closing the
/// picker is the user's choice and says nothing.
Future<void> _sharing(
  BuildContext context,
  Future<void> Function() action,
) async {
  try {
    await action();
  } on RepoException catch (error) {
    if (!context.mounted || error.kind == RepoErrorKind.screenCancelled) {
      return;
    }
    showOcToast(context, switch (error.kind) {
      RepoErrorKind.screenDenied => 'Screen sharing was not allowed.',
      RepoErrorKind.screenUnsupported =>
        "Screen sharing doesn't work on this system yet.",
      RepoErrorKind.qualityLimit => "That's above this server's limit.",
      RepoErrorKind.forbidden =>
        "You don't have permission to share your screen here.",
      RepoErrorKind.rateLimited => 'Wait a moment before going live again.',
      _ => error.message,
    });
  }
}

/// Starts watching a stream, which opens it large (§9.5); a full one
/// says so.
Future<void> watchStream(
  BuildContext context,
  WidgetRef ref,
  String key, {
  VoidCallback? opened,
}) async {
  try {
    await ref.read(watchingProvider.notifier).watch(key);
    opened?.call();
  } on RepoException catch (error) {
    if (!context.mounted) return;
    showOcToast(context, error.message);
  }
}

/// Leaves voice; the toast's Undo joins the same channel again (§16).
Future<void> leaveVoice(BuildContext context, WidgetRef ref) async {
  final voice = ref.read(voiceSessionProvider);
  final server = voice.serverKey;
  final channel = voice.channelId;
  final session = ref.read(voiceSessionProvider.notifier);
  if (server == null || channel == null) return;
  final name = ref.read(serverProvider(server)).data?.channels[channel]?.name;
  // The toast first: leaving may take the button that asked, and [context]
  // with it, away.
  final toast = ocToaster(context);
  toast(
    name == null ? 'Left voice' : 'Left $name',
    actionLabel: 'Undo',
    onAction: () => _joinOrSay(session, toast, server, channel),
  );
  await session.leave();
}

/// The voice view's control bar (§4.10): floats inside the voice view as [VoiceDock].
class VoiceControlBar extends StatelessWidget {
  const VoiceControlBar({super.key, required this.channel, this.chat = false});

  final ChannelRef channel;

  /// The channel's text chat is on (§5.3).
  final bool chat;

  @override
  Widget build(BuildContext context) {
    return VoiceDock(channel: channel, chat: chat);
  }
}


