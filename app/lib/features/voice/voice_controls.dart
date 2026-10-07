import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/voice/screenshare_picker.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Stops sharing, or asks what to share first (§4.10).
Future<void> toggleScreenshare(BuildContext context, WidgetRef ref) async {
  final session = ref.read(voiceSessionProvider.notifier);
  if (ref.read(voiceSessionProvider).screensharing) {
    return session.toggleScreenshare();
  }
  final choice = await showScreensharePicker(context);
  if (choice == null || !context.mounted) return;
  // The call may have ended, or sharing begun elsewhere, meanwhile.
  final voice = ref.read(voiceSessionProvider);
  if (!voice.connected || voice.screensharing) return;
  await session.toggleScreenshare();
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
    onAction: () => session.join(server, channel),
  );
  await session.leave();
}

/// The voice view's control bar (§4.10): Mute · Camera · Screenshare ·
/// Deafen, then Disconnect.
class VoiceControlBar extends ConsumerWidget {
  const VoiceControlBar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final voice = ref.watch(voiceSessionProvider);
    final session = ref.read(voiceSessionProvider.notifier);
    final platform = Theme.of(context).platform;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _RoundToggle(
          icon: OcIcons.mic,
          activeIcon: OcIcons.micOff,
          tooltip: voice.muted ? 'Unmute' : 'Mute',
          shortcut: appShortcut(LogicalKeyboardKey.keyM, platform, shift: true),
          active: voice.muted,
          onPressed: session.toggleMute,
        ),
        const SizedBox(width: OcSpace.s12),
        _RoundToggle(
          icon: OcIcons.videocamOff,
          activeIcon: OcIcons.videocam,
          tooltip: voice.camera ? 'Turn off camera' : 'Turn on camera',
          active: voice.camera,
          onPressed: session.toggleCamera,
        ),
        const SizedBox(width: OcSpace.s12),
        _RoundToggle(
          icon: OcIcons.screenShare,
          activeIcon: OcIcons.stopScreenShare,
          tooltip: voice.screensharing ? 'Stop sharing' : 'Share your screen',
          active: voice.screensharing,
          onPressed: () => toggleScreenshare(context, ref),
        ),
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
