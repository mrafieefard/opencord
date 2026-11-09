import 'package:flutter/material.dart';

import 'package:opencord/core/model/plugins.dart';
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
  fontSize: 11,
  fontWeight: FontWeight.w600,
  height: 1,
);

/// Unread count: an inverted pill, or a grey one when muted (§5.3).
class UnreadBadge extends StatelessWidget {
  const UnreadBadge({
    super.key,
    required this.count,
    this.muted = false,
    this.inverted = false,
  });

  final int count;
  final bool muted;
  final bool inverted;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final color = inverted
        ? colors.onAccent
        : (muted ? colors.selected : colors.accent);
    final textColor = inverted
        ? colors.accent
        : (muted ? colors.textSecondary : colors.onAccent);
    return Container(
      constraints: const BoxConstraints(minWidth: 18),
      height: 18,
      padding: const EdgeInsets.symmetric(horizontal: 5),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(9),
      ),
      child: Text(
        formatCount(count),
        style: _badgeText.copyWith(color: textColor),
      ),
    );
  }
}

/// An inverted "@" pill: someone mentioned you (§5.3).
class MentionBadge extends StatelessWidget {
  const MentionBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      constraints: const BoxConstraints(minWidth: 18),
      height: 18,
      padding: const EdgeInsets.symmetric(horizontal: 5),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: colors.accent,
        borderRadius: BorderRadius.circular(9),
      ),
      child: Text('@', style: _badgeText.copyWith(color: colors.onAccent)),
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
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: colors.accent,
        borderRadius: BorderRadius.circular(5),
      ),
      child: Text(
        'LIVE',
        style: _badgeText.copyWith(
          color: colors.onAccent,
          fontSize: 10.5,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.4,
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

/// A label identifying a bot or plugin author: the app's existing small label style.
class PluginBadge extends StatelessWidget {
  const PluginBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
      decoration: BoxDecoration(
        color: colors.hover,
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: colors.border),
      ),
      child: Text(
        'Plugin',
        maxLines: 1,
        style: OcText.meta.copyWith(
          color: colors.textSecondary,
          fontSize: 10.5,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.2,
        ),
      ),
    );
  }
}

/// A badge for a plugin's signature state: Signed, Verified, Unsigned, Dev.
class PluginSignatureBadge extends StatelessWidget {
  const PluginSignatureBadge({
    super.key,
    required this.signature,
    this.githubCheck = GithubCheck.unspecified,
  });

  final PluginSignature signature;
  final GithubCheck githubCheck;

  String get label {
    if (signature == PluginSignature.dev) return 'Dev';
    if (githubCheck == GithubCheck.verified) return 'Verified';
    if (signature == PluginSignature.signed) return 'Signed';
    return 'Unsigned';
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1.5),
      decoration: BoxDecoration(
        color: colors.hover,
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: colors.border),
      ),
      child: Text(
        label,
        maxLines: 1,
        style: OcText.meta.copyWith(
          color: colors.textSecondary,
          fontSize: 10.5,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.2,
        ),
      ),
    );
  }
}

/// A badge for a plugin's execution state: Running, Stopped, Crashed, Starting.
class PluginStatusBadge extends StatelessWidget {
  const PluginStatusBadge({super.key, required this.state});

  final PluginState state;

  String get label => switch (state) {
    PluginState.running => 'Running',
    PluginState.stopped => 'Stopped',
    PluginState.crashed => 'Crashed',
    PluginState.starting => 'Starting',
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final isRunning = state == PluginState.running;
    final isCrashed = state == PluginState.crashed;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1.5),
      decoration: BoxDecoration(
        color: isRunning ? colors.selected : colors.hover,
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: colors.border),
      ),
      child: Text(
        label,
        maxLines: 1,
        style: OcText.meta.copyWith(
          color: isCrashed ? colors.text : colors.textSecondary,
          fontSize: 10.5,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.2,
        ),
      ),
    );
  }
}

/// A small pill marking newly requested permissions in an update review.
class NewBadge extends StatelessWidget {
  const NewBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
      decoration: BoxDecoration(
        color: colors.accent,
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        'New',
        maxLines: 1,
        style: OcText.meta.copyWith(
          color: colors.onAccent,
          fontSize: 10,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.3,
        ),
      ),
    );
  }
}
