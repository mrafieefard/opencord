import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// Where a private conversation started or moved (§6.3): a dashed line on
/// each side of the server's name. The name opens the server when this device
/// has it.
class RelayDivider extends ConsumerWidget {
  const RelayDivider({super.key, required this.serverName, this.serverKey});

  final String serverName;
  final String? serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final key = serverKey;
    final saved =
        key != null &&
        ref.watch(
          serverListProvider.select(
            (servers) => servers.any((server) => server.key == key),
          ),
        );
    final line = Expanded(
      child: CustomPaint(
        size: const Size.fromHeight(1),
        painter: _DashPainter(color: colors.selected),
      ),
    );
    Widget label(bool hovered) => Text(
      serverName,
      style: OcText.small.copyWith(
        fontSize: 12.5,
        fontWeight: FontWeight.w400,
        color: hovered ? colors.text : colors.textSecondary,
        decoration: hovered ? TextDecoration.underline : null,
      ),
    );
    return Padding(
      padding: const EdgeInsets.only(top: OcSpace.s4, bottom: OcSpace.s8),
      child: Row(
        spacing: OcSpace.s12,
        children: [
          line,
          if (saved)
            Hoverable(
              onTap: () =>
                  ref.read(navigationProvider.notifier).openServer(key),
              semanticLabel: 'Open $serverName',
              focusRadius: BorderRadius.circular(OcRadius.row),
              builder: (context, state) => label(state.active),
            )
          else
            label(false),
          line,
        ],
      ),
    );
  }
}

/// A 1 px dashed line across its box.
class _DashPainter extends CustomPainter {
  const _DashPainter({required this.color});

  final Color color;

  static const _dash = 3.0;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    for (var x = 0.0; x < size.width; x += 2 * _dash) {
      canvas.drawRect(Rect.fromLTWH(x, 0, _dash, size.height), paint);
    }
  }

  @override
  bool shouldRepaint(_DashPainter oldDelegate) => oldDelegate.color != color;
}
