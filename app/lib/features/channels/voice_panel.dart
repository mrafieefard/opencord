import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/voice/camera_menu.dart';
import 'package:opencord/features/voice/connection_info.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/popover.dart';

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
    final capabilities = ref.read(repositoryProvider).capabilities;
    final connection = ref.watch(voiceConnectionProvider);
    final status = connection?.phase.label ?? 'Voice connected';
    final detail = connection?.detail;
    // Rebuilt when the bars change, not with every report.
    final bars = ref.watch(voiceStatsProvider.select((stats) => stats?.bars));
    final quality = bars == null ? null : connectionQuality(bars);
    final rounded = BorderRadius.circular(8);
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
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    // The connection: a click shows its last minute.
                    Builder(
                      builder: (line) => _PanelRow(
                        onTap: () => showPopover<void>(
                          context: context,
                          anchor: globalRectOf(line),
                          side: PopoverSide.right,
                          builder: (_) => const ConnectionGraph(),
                        ),
                        semanticLabel: [
                          status,
                          if (quality != null) 'Connection ${quality.$2}',
                          ?detail,
                          'Show the connection graph',
                        ].join('. '),
                        radius: rounded,
                        child: Row(
                          children: [
                            Icon(
                              quality?.$1 ?? OcIcons.graphicEq,
                              size: OcSize.iconInline,
                              color: colors.text,
                            ),
                            const SizedBox(width: OcSpace.s6),
                            Flexible(
                              child: _StatusText(status, detail: detail),
                            ),
                          ],
                        ),
                      ),
                    ),
                    // The call: a click opens it.
                    _PanelRow(
                      onTap: () => ref
                          .read(navigationProvider.notifier)
                          .openChannel(server, channelId),
                      semanticLabel:
                          '${channel ?? 'Voice'}, ${serverName ?? ''}. '
                          'Open the voice view',
                      radius: rounded,
                      child: Text(
                        '${channel ?? ''} / ${serverName ?? ''}',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(color: colors.textMuted),
                      ),
                    ),
                  ],
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
          if (capabilities.camera ||
              capabilities.screenShare ||
              capabilities.soundboard) ...[
            const SizedBox(height: OcSpace.s8),
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
                  if (index > 0) const SizedBox(width: OcSpace.s8),
                  Expanded(child: toggle),
                ],
              ],
            ),
          ],
        ],
      ),
    );
  }
}

/// One of the panel's two clickable lines, highlighted on hover.
class _PanelRow extends StatelessWidget {
  const _PanelRow({
    required this.onTap,
    required this.semanticLabel,
    required this.radius,
    required this.child,
  });

  final VoidCallback onTap;
  final String semanticLabel;
  final BorderRadius radius;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: semanticLabel,
      focusRadius: radius,
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s4,
          vertical: 1,
        ),
        decoration: BoxDecoration(
          color: state.active ? colors.hover : null,
          borderRadius: radius,
        ),
        child: child,
      ),
    );
  }
}

/// An equal-width icon toggle, inverted while on.
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
            child: Center(
              child: Icon(icon, size: OcSize.iconInline, color: foreground),
            ),
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
      style: OcText.bodyStrong.copyWith(fontSize: 13, color: context.oc.text),
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
