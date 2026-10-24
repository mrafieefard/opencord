import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// Where a private conversation started or moved (§6.3): a wavy line on
/// each side of the server's icon and name. The name opens the server when
/// this device has it.
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
    final wave = Expanded(
      child: CustomPaint(
        size: const Size.fromHeight(_waveHeight),
        painter: _WavePainter(color: colors.textMuted),
      ),
    );
    Widget label(bool hovered) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        ServerAvatar(
          serverKey: key ?? '',
          name: serverName,
          size: 16,
          borderRadius: 4,
        ),
        const SizedBox(width: OcSpace.s6),
        Text(
          serverName,
          style: OcText.small.copyWith(
            color: hovered ? colors.text : colors.textSecondary,
            fontWeight: FontWeight.w500,
            decoration: hovered ? TextDecoration.underline : null,
          ),
        ),
      ],
    );
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s12,
      ),
      child: Row(
        children: [
          wave,
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s10),
            child: saved
                ? Hoverable(
                    onTap: () =>
                        ref.read(navigationProvider.notifier).openServer(key),
                    semanticLabel: 'Open $serverName',
                    focusRadius: BorderRadius.circular(OcRadius.row),
                    builder: (context, state) => Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: OcSpace.s4,
                        vertical: OcSpace.s2,
                      ),
                      child: label(state.active),
                    ),
                  )
                : Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s4,
                      vertical: OcSpace.s2,
                    ),
                    child: label(false),
                  ),
          ),
          wave,
        ],
      ),
    );
  }
}

const double _waveHeight = 8;

/// A thin sine wave across the middle of its box.
class _WavePainter extends CustomPainter {
  const _WavePainter({required this.color});

  final Color color;

  static const _amplitude = 2.5;
  static const _wavelength = 10.0;

  @override
  void paint(Canvas canvas, Size size) {
    final middle = size.height / 2;
    final path = Path()..moveTo(0, middle);
    for (var x = 1.0; x <= size.width; x += 1) {
      path.lineTo(
        x,
        middle + _amplitude * math.sin(2 * math.pi * x / _wavelength),
      );
    }
    canvas.drawPath(
      path,
      Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.2
        ..isAntiAlias = true,
    );
  }

  @override
  bool shouldRepaint(_WavePainter oldDelegate) => oldDelegate.color != color;
}
