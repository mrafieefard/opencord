import 'package:flutter/material.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// Up to two letters: the first letters of the first and last word.
String initialsOf(String name) {
  final words = name.trim().split(RegExp(r'\s+'));
  if (words.first.isEmpty) return '?';
  final first = words.first.characters.first;
  final last = words.length > 1 ? words.last.characters.first : '';
  return (first + last).toUpperCase();
}

/// Initials on a grey picked from the id (§5.2). Users are circles; servers
/// and channels are rounded squares.
class OcAvatar extends StatelessWidget {
  const OcAvatar({
    super.key,
    required this.id,
    required this.name,
    this.size = 36,
    this.borderRadius,
    this.inverted = false,
    this.presence,
    this.speaking = false,
    this.ringColor,
  });

  /// Picks the grey, so the same user or server always looks the same.
  final Object id;
  final String name;
  final double size;

  /// Rounded square with this radius; a circle when null.
  final double? borderRadius;

  /// Accent background and onAccent letters, for selected servers.
  final bool inverted;
  final Presence? presence;

  /// A 2 px ring in the text color (§5.3).
  final bool speaking;

  /// The color behind the avatar, used for the presence badge's ring.
  final Color? ringColor;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final shape = borderRadius == null
        ? const CircleBorder()
        : RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(borderRadius!),
          );
    final avatar = Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: ShapeDecoration(
        shape: shape,
        color: inverted ? colors.accent : colors.avatarShade(id),
      ),
      foregroundDecoration: speaking
          ? ShapeDecoration(
              shape: shape.copyWith(
                side: BorderSide(color: colors.text, width: 2),
              ),
            )
          : null,
      child: Text(
        initialsOf(name),
        maxLines: 1,
        overflow: TextOverflow.clip,
        textScaler: TextScaler.noScaling,
        style: OcText.bodyStrong.copyWith(
          fontSize: size * 0.38,
          height: 1,
          color: inverted ? colors.onAccent : colors.text,
        ),
      ),
    );
    final presence = this.presence;
    if (presence == null) return avatar;
    final badge = presenceBadgeSize(size);
    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          avatar,
          Positioned(
            right: -PresenceBadge.ring,
            bottom: -PresenceBadge.ring,
            child: PresenceBadge(
              presence: presence,
              size: badge,
              ringColor: ringColor ?? colors.sidebar,
            ),
          ),
        ],
      ),
    );
  }
}

/// Presence badge diameter for an avatar of [avatarSize].
double presenceBadgeSize(double avatarSize) => avatarSize >= 60
    ? 16
    : avatarSize >= 30
    ? 12
    : 9;

/// Presence drawn as a shape, never a color (§5.3): online is a filled
/// circle, idle has a moon cut out, do not disturb a bar, offline is a ring.
class PresenceBadge extends StatelessWidget {
  const PresenceBadge({
    super.key,
    required this.presence,
    this.size = 12,
    required this.ringColor,
  });

  /// Width of the ring that separates the badge from what is behind it.
  static const double ring = 2.5;

  final Presence presence;
  final double size;
  final Color ringColor;

  @override
  Widget build(BuildContext context) {
    final extent = size + ring * 2;
    return Semantics(
      label: presence.label,
      child: CustomPaint(
        size: Size.square(extent),
        painter: PresencePainter(
          presence: presence,
          color: context.oc.text,
          ringColor: ringColor,
          ring: ring,
        ),
      ),
    );
  }
}

class PresencePainter extends CustomPainter {
  const PresencePainter({
    required this.presence,
    required this.color,
    required this.ringColor,
    this.ring = 0,
  });

  final Presence presence;
  final Color color;
  final Color ringColor;
  final double ring;

  @override
  void paint(Canvas canvas, Size size) {
    final center = size.center(Offset.zero);
    final outer = size.shortestSide / 2;
    final radius = outer - ring;
    if (ring > 0) {
      canvas.drawCircle(center, outer, Paint()..color = ringColor);
    }
    final fill = Paint()
      ..color = color
      ..isAntiAlias = true;
    final disc = Path()
      ..addOval(Rect.fromCircle(center: center, radius: radius));
    switch (presence) {
      case Presence.online:
        canvas.drawPath(disc, fill);
      case Presence.idle:
        final bite = Path()
          ..addOval(
            Rect.fromCircle(
              center: center.translate(-radius * 0.45, -radius * 0.45),
              radius: radius * 0.68,
            ),
          );
        canvas.drawPath(
          Path.combine(PathOperation.difference, disc, bite),
          fill,
        );
      case Presence.doNotDisturb:
        final bar = Path()
          ..addRRect(
            RRect.fromRectAndRadius(
              Rect.fromCenter(
                center: center,
                width: radius * 1.2,
                height: radius * 0.42,
              ),
              Radius.circular(radius * 0.21),
            ),
          );
        canvas.drawPath(
          Path.combine(PathOperation.difference, disc, bar),
          fill,
        );
      case Presence.offline:
        final hole = Path()
          ..addOval(Rect.fromCircle(center: center, radius: radius * 0.5));
        canvas.drawPath(
          Path.combine(PathOperation.difference, disc, hole),
          fill,
        );
    }
  }

  @override
  bool shouldRepaint(PresencePainter old) =>
      old.presence != presence ||
      old.color != color ||
      old.ringColor != ringColor ||
      old.ring != ring;
}
