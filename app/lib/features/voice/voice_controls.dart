import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/voice/camera_menu.dart';
import 'package:opencord/features/voice/screenshare_picker.dart';
import 'package:opencord/features/voice/sound_from.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Joins a voice channel; why not, when the server refuses, shows as a
/// toast.
Future<void> joinVoice(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  int channelId,
) async {
  try {
    await ref.read(voiceSessionProvider.notifier).join(serverKey, channelId);
  } on RepoException catch (error) {
    if (!context.mounted) return;
    showOcToast(context, switch (error.kind) {
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
  // The toast first: leaving may take the button that asked away.
  showOcToast(
    context,
    name == null ? 'Left voice' : 'Left $name',
    actionLabel: 'Undo',
    onAction: () => joinVoice(context, ref, server, channel),
  );
  await session.leave();
}

/// The voice view's control bar (§4.10): the channel's chat when it has
/// one, then Mute · Camera · Screenshare · Soundboard · Deafen, then
/// Disconnect.
class VoiceControlBar extends ConsumerWidget {
  const VoiceControlBar({super.key, required this.channel, this.chat = false});

  final ChannelRef channel;

  /// The channel's text chat is on (§5.3).
  final bool chat;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final voice = ref.watch(voiceSessionProvider);
    final session = ref.read(voiceSessionProvider.notifier);
    final capabilities = ref.read(repositoryProvider).capabilities;
    final platform = Theme.of(context).platform;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (chat) ...[
          VoiceChatButton(channel: channel, round: true),
          const SizedBox(width: OcSpace.s12),
          Container(width: 1, height: 28, color: context.oc.border),
          const SizedBox(width: OcSpace.s12),
        ],
        _RoundToggle(
          icon: OcIcons.mic,
          activeIcon: OcIcons.micOff,
          tooltip: voice.muted ? 'Unmute' : 'Mute',
          shortcut: appShortcut(LogicalKeyboardKey.keyM, platform, shift: true),
          active: voice.muted,
          onPressed: session.toggleMute,
        ),
        if (capabilities.camera) ...[
          const SizedBox(width: OcSpace.s12),
          // Right-click: which camera.
          GestureDetector(
            onSecondaryTapUp: (details) =>
                showCameraMenu(context, ref, details.globalPosition),
            child: _RoundToggle(
              icon: OcIcons.videocamOff,
              activeIcon: OcIcons.videocam,
              tooltip: voice.camera ? 'Turn off camera' : 'Turn on camera',
              active: voice.camera,
              onPressed: () => toggleCamera(context, ref),
            ),
          ),
        ],
        if (capabilities.screenShare) ...[
          const SizedBox(width: OcSpace.s12),
          Builder(
            builder: (button) => _RoundToggle(
              icon: OcIcons.screenShare,
              activeIcon: OcIcons.screenShare,
              tooltip: voice.screensharing
                  ? 'Screen share options'
                  : 'Share your screen',
              active: voice.screensharing,
              onPressed: () => screenshareControl(
                context,
                ref,
                anchor: globalRectOf(button),
              ),
            ),
          ),
        ],
        if (capabilities.soundboard) ...[
          const SizedBox(width: OcSpace.s12),
          Builder(
            builder: (button) => _RoundToggle(
              icon: OcIcons.musicNote,
              activeIcon: OcIcons.musicNote,
              tooltip: 'Soundboard',
              shortcut: appShortcut(
                LogicalKeyboardKey.keyB,
                platform,
                shift: true,
              ),
              active: false,
              onPressed: () =>
                  showSoundboardPicker(context, anchor: globalRectOf(button)),
            ),
          ),
        ],
        const SizedBox(width: OcSpace.s12),
        _RoundToggle(
          icon: OcIcons.headphones,
          activeIcon: OcIcons.headsetOff,
          tooltip: voice.deafened ? 'Undeafen' : 'Deafen',
          shortcut: appShortcut(LogicalKeyboardKey.keyD, platform, shift: true),
          active: voice.deafened,
          onPressed: session.toggleDeafen,
        ),
        const SizedBox(width: OcSpace.s24),
        _DisconnectPill(onPressed: () => leaveVoice(context, ref)),
      ],
    );
  }
}

/// A voice channel's chat beside its tiles, on or off (Phase 2 §12), with
/// how much is unread there while it is off. [round] is the control bar's
/// circle; otherwise a header button.
class VoiceChatButton extends ConsumerWidget {
  const VoiceChatButton({super.key, required this.channel, this.round = false});

  final ChannelRef channel;
  final bool round;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final open = ref.watch(
      voiceViewPrefsProvider.select((prefs) => prefs.chatOpen),
    );
    final unread = ref.watch(
      activityProvider(
        channel.server,
      ).select((activity) => activity.of(channel.channel).read.unread),
    );
    final toggle = ref.read(voiceViewPrefsProvider.notifier).toggleChat;
    final tooltip = switch ((open, unread)) {
      (true, _) => 'Hide chat',
      (false, 0) => 'Show chat',
      (false, final count) => 'Show chat, $count unread',
    };
    return Stack(
      clipBehavior: Clip.none,
      children: [
        if (round)
          _RoundToggle(
            icon: OcIcons.chat,
            activeIcon: OcIcons.chat,
            tooltip: tooltip,
            active: open,
            onPressed: toggle,
          )
        else
          OcIconButton(
            icon: OcIcons.chat,
            tooltip: tooltip,
            active: open,
            onPressed: toggle,
          ),
        if (!open && unread > 0)
          Positioned(
            right: 0,
            top: 0,
            child: IgnorePointer(child: UnreadBadge(count: unread)),
          ),
      ],
    );
  }
}

/// A 48 px circle, inverted while on.
class _RoundToggle extends StatelessWidget {
  const _RoundToggle({
    required this.icon,
    required this.activeIcon,
    required this.tooltip,
    required this.active,
    required this.onPressed,
    this.shortcut,
  });

  final IconData icon;
  final IconData activeIcon;
  final String tooltip;
  final bool active;
  final VoidCallback onPressed;
  final SingleActivator? shortcut;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return DecoratedBox(
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: colors.surface,
        border: Border.all(color: active ? colors.accent : colors.border),
      ),
      child: OcIconButton(
        icon: icon,
        activeIcon: activeIcon,
        tooltip: tooltip,
        shortcut: shortcut,
        size: OcIconButtonSize.voice,
        active: active,
        activeStyle: OcActiveStyle.inverted,
        color: colors.text,
        onPressed: onPressed,
      ),
    );
  }
}

class _DisconnectPill extends StatelessWidget {
  const _DisconnectPill({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onPressed,
      semanticLabel: 'Disconnect',
      focusRadius: BorderRadius.circular(OcSize.hitVoice / 2),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: OcSize.hitVoice,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s20),
        decoration: BoxDecoration(
          color: state.active
              ? Color.lerp(colors.accent, colors.onAccent, 0.12)
              : colors.accent,
          borderRadius: BorderRadius.circular(OcSize.hitVoice / 2),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(OcIcons.callEnd, size: 22, color: colors.onAccent),
            const SizedBox(width: OcSpace.s8),
            Text(
              'Disconnect',
              style: OcText.bodyStrong.copyWith(color: colors.onAccent),
            ),
          ],
        ),
      ),
    );
  }
}
