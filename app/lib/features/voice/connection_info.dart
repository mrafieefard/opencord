import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// The voice connection's signal bars (Phase 2 §12); a click shows the
/// numbers behind them. Nothing until the first report.
class ConnectionBars extends ConsumerWidget {
  const ConnectionBars({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final stats = ref.watch(voiceStatsProvider);
    if (stats == null) return const SizedBox.shrink();
    final (icon, quality) = switch (stats.bars) {
      3 => (OcIcons.signalCellularAlt, 'good'),
      2 => (OcIcons.signalCellularAlt2Bar, 'fair'),
      _ => (OcIcons.signalCellularAlt1Bar, 'poor'),
    };
    return Builder(
      builder: (button) => OcIconButton(
        icon: icon,
        tooltip: 'Connection info, $quality',
        size: OcIconButtonSize.compact,
        onPressed: () => showPopover<void>(
          context: context,
          anchor: globalRectOf(button),
          side: PopoverSide.right,
          builder: (_) => const ConnectionInfo(),
        ),
      ),
    );
  }
}

/// What the voice connection measured over the last second, as it comes.
class ConnectionInfo extends ConsumerWidget {
  const ConnectionInfo({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final stats = ref.watch(voiceStatsProvider);
    if (stats == null) {
      return Text(
        'Not connected to voice.',
        style: OcText.small.copyWith(color: colors.textSecondary),
      );
    }
    String percent(double? value) =>
        value == null ? '–' : '${value.toStringAsFixed(1)} %';
    final rows = <(String, String)>[
      ('Ping', stats.pingMs == null ? '–' : '${stats.pingMs} ms'),
      (
        'Jitter',
        stats.jitterMs == null
            ? '–'
            : '${stats.jitterMs!.toStringAsFixed(1)} ms',
      ),
      ('Packet loss in', percent(stats.lossInPercent)),
      ('Packet loss out', percent(stats.lossOutPercent)),
      ('Received', formatBitrate(stats.bitrateIn)),
      ('Sent', formatBitrate(stats.bitrateOut)),
      ('Voice node', stats.voiceNode),
      ('Voice codec', 'Opus'),
      if (stats.cameraEncoder case final encoder?)
        ('Camera encoder', 'H.264, $encoder'),
      if (stats.screenEncoder case final encoder?)
        ('Screen encoder', 'H.264, $encoder'),
    ];
    return ConstrainedBox(
      constraints: const BoxConstraints(minWidth: 240, maxWidth: 300),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Connection info',
            style: OcText.bodyStrong.copyWith(color: colors.text),
          ),
          const SizedBox(height: OcSpace.s8),
          for (final (label, value) in rows)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      label,
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  ),
                  const SizedBox(width: OcSpace.s12),
                  Flexible(
                    child: Text(
                      value,
                      textAlign: TextAlign.end,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.small.copyWith(color: colors.text),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Bits per second as people read them: "64 kbps", "1.2 Mbps".
String formatBitrate(int bitsPerSecond) {
  if (bitsPerSecond >= 1000000) {
    return '${(bitsPerSecond / 1000000).toStringAsFixed(1)} Mbps';
  }
  return '${(bitsPerSecond / 1000).round()} kbps';
}
