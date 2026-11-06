import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/voice/voice_recorder_state.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/voice/camera_menu.dart';
import 'package:opencord/features/voice/connection_info.dart';
import 'package:opencord/features/voice/noise_chip.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// The voice connected panel (§5.5), above the user panel while in voice.
class VoiceConnectedPanel extends ConsumerWidget {
  const VoiceConnectedPanel({
    super.key,
    this.serverKey,
    this.channelId,
    this.inVoice = true,
  });

  final String? serverKey;
  final int? channelId;
  final bool inVoice;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final voice = ref.watch(voiceSessionProvider);
    final server = serverKey ?? voice.serverKey;
    final chId = channelId ?? voice.channelId;
    if (server == null || chId == null) return const SizedBox.shrink();
    final colors = context.oc;
    final channel = ref.watch(
      serverProvider(server).select((s) => s.data?.channels[chId]?.name),
    );
    final serverName = ref.watch(
      serverProvider(server).select((s) => s.data?.info.name),
    );
    final capabilities = ref.read(repositoryProvider).capabilities;
    final connection = ref.watch(voiceConnectionProvider);
    final isRecordingVoice = ref.watch(isRecordingVoiceProvider);
    final status = isRecordingVoice
        ? 'Recording a voice message'
        : (connection?.phase.label ?? 'Voice connected');
    final detail = connection?.detail;
    // Rebuilt when the bars change, not with every report.
    final bars = ref.watch(voiceStatsProvider.select((stats) => stats?.bars));
    final quality = bars == null ? null : connectionQuality(bars);
    final rounded = BorderRadius.circular(8);
    return IgnorePointer(
      // On its way out it only shows the channel it was in.
      ignoring: !inVoice,
      child: Container(
        padding: const EdgeInsets.fromLTRB(8, 8, 8, 10),
        decoration: BoxDecoration(color: colors.sidebar),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(
                  child: Hoverable(
                    onTap: () => ref
                        .read(navigationProvider.notifier)
                        .openChannel(server, chId),
                    onSecondaryTap: (pos) => showPopover<void>(
                      context: context,
                      anchor: Rect.fromLTWH(pos.dx, pos.dy, 1, 1),
                      side: PopoverSide.right,
                      builder: (_) => const ConnectionGraph(),
                    ),
                    focusRadius: rounded,
                    builder: (context, state) => AnimatedContainer(
                      duration: OcMotion.of(context).hover,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 6,
                        vertical: 2,
                      ),
                      decoration: BoxDecoration(
                        color: state.active ? colors.hover : null,
                        borderRadius: rounded,
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Builder(
                            builder: (line) => GestureDetector(
                              behavior: HitTestBehavior.opaque,
                              onTap: () => showPopover<void>(
                                context: context,
                                anchor: globalRectOf(line),
                                side: PopoverSide.right,
                                builder: (_) => const ConnectionGraph(),
                              ),
                              child: Semantics(
                                label: [
                                  status,
                                  if (quality != null)
                                    'Connection ${quality.$2}',
                                  ?detail,
                                  'Show the connection graph',
                                ].join('. '),
                                button: true,
                                child: Row(
                                  children: [
                                    Icon(
                                      quality?.$1 ?? OcIcons.graphicEq,
                                      size: 16,
                                      color: colors.text,
                                    ),
                                    const SizedBox(width: 6),
                                    Flexible(
                                      child: _StatusText(
                                        status,
                                        detail: detail,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ),
                          const SizedBox(height: 1),
                          Semantics(
                            label:
                                '${channel ?? 'Voice'}, ${serverName ?? ''}. Open the voice view',
                            button: true,
                            child: Text(
                              '${channel ?? ''} / ${serverName ?? ''}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.meta.copyWith(
                                fontSize: 12.5,
                                height: 1.35,
                                color: colors.textMuted,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 6),
                const NoiseChip(),
                const SizedBox(width: 6),
                OcIconButton(
                  icon: OcIcons.callEnd,
                  iconSize: 20,
                  tooltip: 'Disconnect',
                  size: OcIconButtonSize.compact,
                  color: colors.textSecondary,
                  onPressed: () => leaveVoice(context, ref),
                ),
              ],
            ),
            if (capabilities.camera ||
                capabilities.screenShare ||
                capabilities.soundboard) ...[
              const SizedBox(height: 8),
              Row(
                children: [
                  for (final (index, toggle) in [
                    if (capabilities.camera)
                      _Toggle(
                        icon: voice.camera
                            ? OcIcons.videocam
                            : OcIcons.videocamOff,
                        label: 'Camera',
                        active: voice.camera,
                        onTap: () => toggleCamera(context, ref),
                        onSecondaryTap: (position) =>
                            showCameraMenu(context, ref, position),
                      ),
                    if (capabilities.screenShare)
                      Builder(
                        builder: (button) => _Toggle(
                          icon: OcIcons.screenShare,
                          label: voice.screensharing ? 'Live' : 'Screen',
                          active: voice.screensharing,
                          onTap: () => screenshareControl(
                            context,
                            ref,
                            anchor: globalRectOf(button),
                          ),
                        ),
                      ),
                    if (capabilities.soundboard)
                      Builder(
                        builder: (button) => _Toggle(
                          icon: OcIcons.musicNote,
                          label: 'Soundboard',
                          active: false,
                          onTap: () => showSoundboardPicker(
                            context,
                            anchor: globalRectOf(button),
                            side: PopoverSide.right,
                          ),
                        ),
                      ),
                  ].indexed) ...[
                    if (index > 0) const SizedBox(width: 8),
                    Expanded(child: toggle),
                  ],
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// An equal-width icon toggle, inverted while on (§5.5).
class _Toggle extends StatelessWidget {
  const _Toggle({
    required this.icon,
    required this.label,
    required this.active,
    required this.onTap,
    this.onSecondaryTap,
  });

  final IconData icon;
  final String label;
  final bool active;
  final VoidCallback onTap;
  final ValueChanged<Offset>? onSecondaryTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    // Icons only: the name shows on hover.
    return Tooltip(
      message: label,
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: onTap,
        onSecondaryTap: onSecondaryTap,
        semanticLabel: label,
        selected: active,
        focusRadius: BorderRadius.circular(10),
        builder: (context, state) {
          final foreground = active
              ? colors.onAccent
              : (state.active ? colors.text : colors.textSecondary);
          return AnimatedContainer(
            duration: OcMotion.of(context).hover,
            height: 36,
            decoration: BoxDecoration(
              color: active
                  ? colors.accent
                  : state.active
                  ? colors.selected
                  : colors.hover,
              borderRadius: BorderRadius.circular(10),
            ),
            child: Center(child: Icon(icon, size: 20, color: foreground)),
          );
        },
      ),
    );
  }
}

/// The connection's phase, with more about it on hover when there is more.
class _StatusText extends StatelessWidget {
  const _StatusText(this.status, {this.detail});

  final String status;
  final String? detail;

  @override
  Widget build(BuildContext context) {
    final text = Text(
      status,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: OcText.header.copyWith(
        fontSize: 13,
        fontWeight: FontWeight.w600,
        height: 1.35,
        color: context.oc.text,
      ),
    );
    return switch (detail) {
      final detail? => Tooltip(
        message: detail,
        excludeFromSemantics: true,
        child: text,
      ),
      null => text,
    };
  }
}
