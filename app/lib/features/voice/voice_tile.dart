import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:opencord/core/model/video.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// One tile of the voice view (§4.10): someone, or the screen they share.
/// It fills the size its parent gives it, which is 16:9.
class VoiceTileView extends StatelessWidget {
  const VoiceTileView({
    super.key,
    required this.userId,
    required this.name,
    this.screen = false,
    this.muted = false,
    this.deafened = false,
    this.camera = false,
    this.video,
    this.speaking = false,
    this.focused = false,
    this.small = false,
    this.onTap,
    this.onWatch,
    this.onStopWatching,
    this.viewers,
    this.onViewers,
  });

  final int userId;
  final String name;

  /// The screen [name] shares, not [name] themselves.
  final bool screen;
  final bool muted;
  final bool deafened;
  final bool camera;

  /// Their camera's video, when there is some to draw.
  final VideoFeed? video;
  final bool speaking;

  /// Shown large in focus mode.
  final bool focused;

  /// In focus mode's strip.
  final bool small;

  /// Focuses the tile, or leaves focus mode when it is [focused].
  final VoidCallback? onTap;

  /// Someone else's screen, not watched yet: nothing of it comes until
  /// they click Watch (§9.5).
  final VoidCallback? onWatch;

  /// A watched screen, shown large: stops watching it.
  final VoidCallback? onStopWatching;

  /// This device's own screen: how many watch, and who.
  final int? viewers;
  final VoidCallback? onViewers;

  String get _semanticLabel => [
    screen ? "$name's screen, live" : name,
    if (!screen && camera) 'camera on',
    if (!screen && deafened) 'deafened' else if (!screen && muted) 'muted',
    if (!screen && speaking) 'speaking',
  ].join(', ');

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final radius = BorderRadius.circular(OcRadius.tile);
    return Hoverable(
      onTap: onTap,
      semanticLabel: _semanticLabel,
      focusRadius: radius,
      builder: (context, state) => Container(
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(
          color: screen ? colors.rail : colors.surface,
          borderRadius: radius,
          border: Border.all(color: colors.border),
        ),
        // Drawn over the content so speaking never moves anything.
        foregroundDecoration: speaking && !screen
            ? BoxDecoration(
                borderRadius: radius,
                border: Border.all(color: colors.text, width: 2),
              )
            : null,
        child: Stack(
          fit: StackFit.expand,
          children: [
            if (screen)
              if (video case final feed? when feed.textureId != null)
                // Shared text stays whole: fitted, never cropped.
                VideoFeedView(feed: feed, fit: BoxFit.contain)
              else if (onWatch case final watch?)
                _WatchPrompt(name: name, small: small, onWatch: watch)
              else
                const ScreenFeed()
            else if (video case final feed? when feed.textureId != null)
              VideoFeedView(feed: feed)
            else if (camera)
              CustomPaint(painter: _CameraFeed(colors))
            else
              // At most 72 px, and never into the name pill on small tiles.
              LayoutBuilder(
                builder: (context, constraints) => Center(
                  child: OcAvatar(
                    id: userId,
                    name: name,
                    size: small
                        ? 36
                        : math.min(72, constraints.maxHeight * 0.5),
                  ),
                ),
              ),
            Positioned(
              left: small ? OcSpace.s4 : OcSpace.s8,
              right: small ? OcSpace.s4 : 56,
              bottom: small ? OcSpace.s4 : OcSpace.s8,
              child: Align(
                alignment: Alignment.bottomLeft,
                child: _NamePill(
                  name: name,
                  icon: screen
                      ? OcIcons.monitor
                      : deafened
                      ? OcIcons.headsetOff
                      : muted
                      ? OcIcons.micOff
                      : null,
                  small: small,
                ),
              ),
            ),
            Positioned(
              top: small ? OcSpace.s4 : OcSpace.s8,
              right: small ? OcSpace.s4 : OcSpace.s8,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (!small)
                    AnimatedOpacity(
                      opacity: state.active ? 1 : 0,
                      duration: OcMotion.of(context).hover,
                      child: _CornerIcon(
                        icon: focused
                            ? OcIcons.closeFullscreen
                            : OcIcons.openInFull,
                      ),
                    ),
                  if (screen) ...[
                    const SizedBox(width: OcSpace.s6),
                    _LivePill(small: small),
                  ],
                ],
              ),
            ),
            if (viewers case final count? when !small)
              Positioned(
                top: OcSpace.s8,
                left: OcSpace.s8,
                child: _ViewersPill(count: count, onTap: onViewers),
              ),
            if (onStopWatching case final stop? when focused)
              Positioned(
                right: OcSpace.s8,
                bottom: OcSpace.s8,
                child: OcButton(label: 'Stop watching', onPressed: stop),
              ),
          ],
        ),
      ),
    );
  }
}

/// A camera's video filling its tile: cropped to the tile's shape, never
/// stretched, mirrored for this device's own preview (Phase 2 plan §8).
class VideoFeedView extends StatelessWidget {
  const VideoFeedView({super.key, required this.feed, this.fit = BoxFit.cover});

  final VideoFeed feed;

  /// Cameras fill their tiles; shared screens fit in them.
  final BoxFit fit;

  @override
  Widget build(BuildContext context) {
    final textureId = feed.textureId;
    if (textureId == null) return const SizedBox.shrink();
    final texture = Texture(
      textureId: textureId,
      filterQuality: FilterQuality.medium,
    );
    return ClipRect(
      child: FittedBox(
        fit: fit,
        child: SizedBox(
          width: feed.width.toDouble(),
          height: feed.height.toDouble(),
          child: feed.mirrored
              ? Transform.flip(flipX: true, child: texture)
              : texture,
        ),
      ),
    );
  }
}

class _NamePill extends StatelessWidget {
  const _NamePill({required this.name, required this.icon, this.small = false});

  final String name;
  final IconData? icon;
  final bool small;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: EdgeInsets.symmetric(
        horizontal: small ? OcSpace.s6 : OcSpace.s8,
        vertical: small ? 2 : OcSpace.s4,
      ),
      decoration: BoxDecoration(
        color: colors.elevated.withValues(alpha: 0.9),
        borderRadius: BorderRadius.circular(OcRadius.reaction),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon case final icon?) ...[
            Icon(icon, size: small ? 12 : 14, color: colors.text),
            const SizedBox(width: OcSpace.s4),
          ],
          Flexible(
            child: Text(
              name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: (small ? OcText.meta : OcText.small).copyWith(
                fontWeight: FontWeight.w600,
                color: colors.text,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Someone's screen before watching it (§9.5): they are live, and Watch.
class _WatchPrompt extends StatelessWidget {
  const _WatchPrompt({
    required this.name,
    required this.small,
    required this.onWatch,
  });

  final String name;
  final bool small;
  final VoidCallback onWatch;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    if (small) {
      return Center(child: Icon(OcIcons.monitor, color: colors.textMuted));
    }
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            '$name is live',
            style: OcText.bodyStrong.copyWith(color: colors.text),
          ),
          const SizedBox(height: OcSpace.s8),
          OcButton.primary(label: 'Watch', onPressed: onWatch),
        ],
      ),
    );
  }
}

/// How many watch this device's own stream; opens who.
class _ViewersPill extends StatelessWidget {
  const _ViewersPill({required this.count, required this.onTap});

  final int count;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: count == 1 ? '1 viewer' : '$count viewers',
      focusRadius: BorderRadius.circular(OcRadius.reaction),
      builder: (context, state) => Container(
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s8,
          vertical: OcSpace.s4,
        ),
        decoration: BoxDecoration(
          color: state.active
              ? colors.hover
              : colors.elevated.withValues(alpha: 0.9),
          borderRadius: BorderRadius.circular(OcRadius.reaction),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(OcIcons.visibility, size: 14, color: colors.text),
            const SizedBox(width: OcSpace.s4),
            Text(
              '$count',
              style: OcText.small.copyWith(
                fontWeight: FontWeight.w600,
                color: colors.text,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// The inverted LIVE mark of a screenshare tile.
class _LivePill extends StatelessWidget {
  const _LivePill({this.small = false});

  final bool small;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: EdgeInsets.symmetric(
        horizontal: small ? OcSpace.s4 : OcSpace.s6,
        vertical: 2,
      ),
      decoration: BoxDecoration(
        color: colors.accent,
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text(
        'LIVE',
        style: OcText.label.copyWith(
          fontSize: small ? 9 : 10.5,
          color: colors.onAccent,
        ),
      ),
    );
  }
}

class _CornerIcon extends StatelessWidget {
  const _CornerIcon({required this.icon});

  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      width: 28,
      height: 28,
      decoration: BoxDecoration(
        color: colors.elevated.withValues(alpha: 0.9),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Icon(icon, size: 16, color: colors.text),
    );
  }
}

/// A stand-in for camera video until Phase 2: a head and shoulders.
class _CameraFeed extends CustomPainter {
  _CameraFeed(this.colors);

  final OcColors colors;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    canvas.drawRect(
      rect,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [colors.hover, colors.rail],
        ).createShader(rect),
    );
    final figure = Paint()..color = colors.textMuted.withValues(alpha: 0.35);
    final unit = size.height;
    canvas.drawCircle(Offset(size.width / 2, unit * 0.42), unit * 0.16, figure);
    canvas.drawRRect(
      RRect.fromRectAndCorners(
        Rect.fromCenter(
          center: Offset(size.width / 2, unit * 0.92),
          width: unit * 0.78,
          height: unit * 0.5,
        ),
        topLeft: Radius.circular(unit * 0.22),
        topRight: Radius.circular(unit * 0.22),
      ),
      figure,
    );
  }

  @override
  bool shouldRepaint(_CameraFeed oldDelegate) => oldDelegate.colors != colors;
}

/// A stand-in for a shared screen until Phase 2: a window with lines.
class ScreenFeed extends StatelessWidget {
  const ScreenFeed({super.key});

  @override
  Widget build(BuildContext context) =>
      CustomPaint(painter: _ScreenPainter(context.oc));
}

class _ScreenPainter extends CustomPainter {
  _ScreenPainter(this.colors);

  final OcColors colors;

  @override
  void paint(Canvas canvas, Size size) {
    final window = Rect.fromLTWH(
      size.width * 0.1,
      size.height * 0.12,
      size.width * 0.8,
      size.height * 0.76,
    );
    final corner = Radius.circular(size.height * 0.03);
    canvas.drawRRect(
      RRect.fromRectAndRadius(window, corner),
      Paint()..color = colors.surface,
    );
    final bar = window.height * 0.1;
    canvas.drawRRect(
      RRect.fromRectAndCorners(
        Rect.fromLTWH(window.left, window.top, window.width, bar),
        topLeft: corner,
        topRight: corner,
      ),
      Paint()..color = colors.hover,
    );
    final line = Paint()..color = colors.selected;
    final lineHeight = window.height * 0.05;
    for (final (index, width) in [0.62, 0.48, 0.7, 0.36, 0.55].indexed) {
      final top = window.top + bar * 1.8 + index * lineHeight * 2.1;
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromLTWH(
            window.left + window.width * 0.06,
            top,
            window.width * width,
            lineHeight,
          ),
          Radius.circular(lineHeight / 2),
        ),
        line,
      );
    }
  }

  @override
  bool shouldRepaint(_ScreenPainter oldDelegate) =>
      oldDelegate.colors != colors;
}
