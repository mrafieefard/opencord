import 'package:flutter/material.dart';

import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// A 28 px hatched band showing where a private conversation started or moved (§6.3).
class RelayDivider extends StatelessWidget {
  const RelayDivider({
    super.key,
    required this.serverName,
    this.serverKey,
  });

  final String serverName;
  final String? serverKey;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    return Container(
      height: 28,
      margin: const EdgeInsets.symmetric(vertical: OcSpace.s8),
      child: Stack(
        alignment: Alignment.center,
        children: [
          CustomPaint(
            size: const Size(double.infinity, 28),
            painter: _HatchPainter(color: colors.border),
          ),
          Container(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s10,
              vertical: OcSpace.s4,
            ),
            decoration: BoxDecoration(
              color: colors.surface,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: colors.border),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                ServerAvatar(
                  serverKey: serverKey ?? '',
                  name: serverName,
                  size: 16,
                  borderRadius: 4,
                ),
                const SizedBox(width: OcSpace.s6),
                Text(
                  serverName,
                  style: OcText.small.copyWith(
                    color: colors.text,
                    fontWeight: FontWeight.w500,
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

class _HatchPainter extends CustomPainter {
  const _HatchPainter({required this.color});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.0;

    const step = 6.0;
    final total = size.width + size.height;
    for (double x = 0; x < total; x += step) {
      canvas.drawLine(
        Offset(x, 0),
        Offset(x - size.height, size.height),
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(_HatchPainter oldDelegate) => oldDelegate.color != color;
}
