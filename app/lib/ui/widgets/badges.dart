import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// `7`, `999`, `1.2K`, `10K`, `1.2M`, like Telegram.
String formatCount(int count) {
  String compact(int value, int unit, String suffix) {
    final whole = value ~/ unit;
    if (whole >= 10) return '$whole$suffix';
    final tenth = (value % unit) * 10 ~/ unit;
    return tenth == 0 ? '$whole$suffix' : '$whole.$tenth$suffix';
  }

  if (count < 1000) return '$count';
  if (count < 1000000) return compact(count, 1000, 'K');
  return compact(count, 1000000, 'M');
}

final _badgeText = OcText.meta.copyWith(
  fontSize: 11.5,
  fontWeight: FontWeight.w700,
  height: 1,
);

/// Unread count: an inverted pill, or a grey one when muted (§5.3).
class UnreadBadge extends StatelessWidget {
  const UnreadBadge({super.key, required this.count, this.muted = false});

  final int count;
  final bool muted;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      constraints: const BoxConstraints(minWidth: 20),
      height: 20,
      padding: const EdgeInsets.symmetric(horizontal: 6),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: muted ? colors.selected : colors.accent,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Text(
        formatCount(count),
        style: _badgeText.copyWith(
          color: muted ? colors.textSecondary : colors.onAccent,
        ),
      ),
    );
  }
}

/// An inverted "@" circle: someone mentioned you (§5.3).
class MentionBadge extends StatelessWidget {
  const MentionBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      width: 20,
      height: 20,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: colors.accent, shape: BoxShape.circle),
      child: Text(
        '@',
        style: _badgeText.copyWith(color: colors.onAccent, fontSize: 12),
      ),
    );
  }
}

/// An inverted LIVE pill for screen sharing (§5.3).
class LivePill extends StatelessWidget {
  const LivePill({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      height: 16,
      padding: const EdgeInsets.symmetric(horizontal: 6),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: colors.accent,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(
        'LIVE',
        style: _badgeText.copyWith(
          color: colors.onAccent,
          fontSize: 9.5,
          letterSpacing: 0.6,
        ),
      ),
    );
  }
}

/// Connection state of a server: a filled dot when connected, a hollow one
/// while connecting (§5.3).
class ConnectionDot extends StatelessWidget {
  const ConnectionDot({super.key, required this.connected, this.size = 8});

  final bool connected;
  final double size;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: connected ? colors.text : null,
        border: connected
            ? null
            : Border.all(color: colors.textSecondary, width: 1.5),
      ),
    );
  }
}
