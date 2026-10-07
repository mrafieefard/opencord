import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// The voice connected panel (§4.2), above the user panel while in voice.
class VoiceConnectedPanel extends ConsumerWidget {
  const VoiceConnectedPanel({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final voice = ref.watch(voiceSessionProvider);
    final server = voice.serverKey;
    final channelId = voice.channelId;
    if (server == null || channelId == null) return const SizedBox.shrink();
    final colors = context.oc;
    final channel = ref.watch(
      serverProvider(server).select((s) => s.data?.channels[channelId]?.name),
    );
    final serverName = ref.watch(
      serverProvider(server).select((s) => s.data?.info.name),
    );
    final session = ref.read(voiceSessionProvider.notifier);
    return Container(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s8,
        OcSpace.s8,
        OcSpace.s8,
        OcSpace.s10,
      ),
      decoration: BoxDecoration(
        color: colors.sidebar,
        border: Border(top: BorderSide(color: colors.border)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Expanded(
                child: Hoverable(
                  onTap: () => ref
                      .read(navigationProvider.notifier)
                      .openChannel(server, channelId),
                  semanticLabel:
                      'Voice connected to ${channel ?? 'voice'}. Open the voice view',
                  builder: (context, state) => AnimatedContainer(
                    duration: OcMotion.of(context).hover,
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s4,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      color: state.active ? colors.hover : null,
                      borderRadius: BorderRadius.circular(8),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Row(
                          children: [
                            Icon(
                              OcIcons.graphicEq,
                              size: OcSize.iconInline,
                              color: colors.text,
                            ),
                            const SizedBox(width: OcSpace.s6),
                            Text(
                              'Voice connected',
                              style: OcText.bodyStrong.copyWith(
                                fontSize: 13,
                                color: colors.text,
                              ),
                            ),
                          ],
                        ),
                        Text(
                          '${channel ?? ''} / ${serverName ?? ''}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.small.copyWith(color: colors.textMuted),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              OcIconButton(
                icon: OcIcons.callEnd,
                tooltip: 'Disconnect',
                size: OcIconButtonSize.compact,
                onPressed: () => leaveVoice(context, ref),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s8),
          Row(
            children: [
              Expanded(
                child: _Toggle(
                  icon: voice.camera ? OcIcons.videocam : OcIcons.videocamOff,
                  label: 'Camera',
                  active: voice.camera,
                  onTap: session.toggleCamera,
                ),
              ),
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: _Toggle(
                  icon: voice.screensharing
                      ? OcIcons.stopScreenShare
                      : OcIcons.screenShare,
                  label: 'Screen',
                  active: voice.screensharing,
                  onTap: () => toggleScreenshare(context, ref),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// An equal-width toggle, inverted while on.
class _Toggle extends StatelessWidget {
  const _Toggle({
    required this.icon,
    required this.label,
    required this.active,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: label,
      selected: active,
      focusRadius: BorderRadius.circular(8),
      builder: (context, state) {
        final foreground = active ? colors.onAccent : colors.text;
        return AnimatedContainer(
          duration: OcMotion.of(context).hover,
          height: 32,
          decoration: BoxDecoration(
            color: active
                ? colors.accent
                : state.active
                ? colors.selected
                : colors.hover,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, size: OcSize.iconInline, color: foreground),
              const SizedBox(width: OcSpace.s6),
              Text(
                label,
                style: OcText.small.copyWith(
                  fontWeight: FontWeight.w600,
                  color: foreground,
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}
