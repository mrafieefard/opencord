import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/features/soundboard/soundboard_ring.dart';

import 'package:opencord/ui/motion/flip_box.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/motion/spring_value.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// One tile of the voice view (§5.14): someone, or the screen they share.
/// It fills the size its parent gives it, which is 16:10 in the grid.
class VoiceTileView extends StatelessWidget {
  const VoiceTileView({
    super.key,
    required this.userId,
    required this.name,
    this.screen = false,
    this.muted = false,
    this.deafened = false,
    this.mutedHere = false,
    this.camera = false,
    this.video,
    this.speaking = false,
    this.sound,
    this.focused = false,
    this.small = false,
    this.onTap,
    this.onSecondaryTap,
    this.onWatch,
    this.onStopWatching,
    this.viewers,
    this.onViewers,
    this.picture,
  });

  final int userId;
  final String name;

  /// Their profile picture, when they have one.
  final ImageProvider? picture;

  /// The screen [name] shares, not [name] themselves.
  final bool screen;

  /// By themselves or by a moderator.
  final bool muted;
  final bool deafened;

  /// Muted on this device only (the per-user menu, desktop UI plan §17.3).
  final bool mutedHere;
  final bool camera;

  /// Their camera's video, when there is some to draw.
  final VideoFeed? video;
  final bool speaking;

  /// The soundboard sound they are playing (§11.6): a dashed outer border
  /// and its emoji in the top-left corner.
  final SoundPlay? sound;

  /// Shown large in focus mode.
  final bool focused;

  /// In focus mode's strip.
  final bool small;

  /// Focuses the tile, or leaves focus mode when it is [focused].
  final VoidCallback? onTap;

  /// Opens their member menu at the pointer.
  final ValueChanged<Offset>? onSecondaryTap;

  /// Someone else's screen, not watched yet: nothing of it comes until
  /// they click Watch (§9.5).
  final VoidCallback? onWatch;

  /// A watched screen, shown large: stops watching it.
  final VoidCallback? onStopWatching;

  /// This device's own screen: how many watch, and who.
  final int? viewers;
  final VoidCallback? onViewers;

  static const double _avatar = 72, _smallAvatar = 36, _liveInset = 10;

  /// The prototype's 72 stands on its shortest tile, 119.7 inside its border
  /// at 1280; only a tile shorter than that gets a smaller one.
  static const double _avatarOfHeight = 0.61;

  String get _semanticLabel => [
    screen ? "$name's screen, live" : name,
    if (screen && mutedHere) 'sound muted by you',
    if (!screen && camera) 'camera on',
    if (!screen && deafened) 'deafened' else if (!screen && muted) 'muted',
    if (!screen && mutedHere) 'muted by you',
    if (!screen && speaking) 'speaking',
    if (!screen && sound != null) 'playing ${sound!.name}',
  ].join(', ');

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final radius = BorderRadius.circular(OcRadius.tile);
    final playing = !screen ? sound : null;
    return Hoverable(
      onTap: onTap,
      onSecondaryTap: onSecondaryTap,
      semanticLabel: _semanticLabel,
      focusRadius: radius,
      builder: (context, state) => SoundboardRing(
        playing: playing != null,
        radius: OcRadius.tile,
        child: Container(
          clipBehavior: Clip.antiAlias,
          decoration: BoxDecoration(
            color: screen ? colors.chat : colors.surface,
            borderRadius: radius,
            border: Border.all(
              color: focused ? colors.selected : colors.border,
            ),
          ),
          // Drawn over the content so speaking never moves anything.
          foregroundDecoration: speaking && !screen
              ? BoxDecoration(
                  borderRadius: radius,
                  border: Border.all(color: colors.text, width: 2),
                )
              : null,
          // The tile's frame scales on the way; this comes back after.
          child: FlipBoxContents(
            child: Stack(
              fit: StackFit.expand,
              children: [
                if (screen)
                  if (video case final feed? when feed.textureId != null)
                    // Shared text stays whole: fitted, never cropped.
                    VideoFeedView(feed: feed, fit: BoxFit.contain)
                  else if (onWatch case final watch?)
                    _WatchPrompt(name: name, small: small, onWatch: watch)
                  else if (!small)
                    const ScreenFeed()
                  else
                    const SizedBox.shrink()
                else if (video case final feed? when feed.textureId != null)
                  VideoFeedView(
                    feed: feed,
                    fit: focused ? BoxFit.contain : BoxFit.cover,
                  )
                else if (camera)
                  const _CameraSilhouette()
                else
                  LayoutBuilder(
                    builder: (context, constraints) => Center(
                      child: OcAvatar(
                        id: '$userId',
                        name: name,
                        image: picture,
                        size: small
                            ? _smallAvatar
                            : math.min(
                                _avatar,
                                constraints.maxHeight * _avatarOfHeight,
                              ),
                      ),
                    ),
                  ),
                Positioned(
                  left: small ? OcSpace.s4 : OcSpace.s8,
                  right: small ? OcSpace.s4 : OcSpace.s8,
                  bottom: small ? OcSpace.s4 : OcSpace.s8,
                  child: Align(
                    alignment: Alignment.bottomLeft,
                    child: _NamePill(
                      name: name,
                      icon: screen
                          ? (mutedHere ? OcIcons.volumeOff : OcIcons.monitor)
                          : deafened
                          ? OcIcons.headsetOff
                          : muted
                          ? OcIcons.micOff
                          : mutedHere
                          ? OcIcons.volumeOff
                          : null,
                      small: small,
                    ),
                  ),
                ),
                if (screen && !small)
                  const Positioned(
                    top: _liveInset,
                    right: _liveInset,
                    child: _LivePill(),
                  ),
                if (viewers case final count? when !small)
                  Positioned(
                    top: OcSpace.s8,
                    left: OcSpace.s8,
                    child: _ViewersPill(count: count, onTap: onViewers),
                  ),
                if (playing?.emoji case final emoji?)
                  Positioned(
                    top: small ? OcSpace.s4 : OcSpace.s8,
                    left: small ? OcSpace.s4 : OcSpace.s8,
                    child: ExcludeSemantics(
                      child: Text(
                        emoji,
                        style: TextStyle(fontSize: small ? 14 : 20),
                      ),
                    ),
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
        horizontal: small ? 6 : 9,
        vertical: small ? 1 : 3,
      ),
      decoration: BoxDecoration(
        color: colors.sidebar,
        borderRadius: BorderRadius.circular(OcRadius.reaction),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 5,
        children: [
          if (icon case final icon?) Icon(icon, size: 16, color: colors.text),
          Flexible(
            child: Text(
              name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: OcText.small.copyWith(
                fontSize: small ? 11 : 12.5,
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

  /// The words, the gap and the button. A tile shorter than this says nothing:
  /// a click on it opens it large, with the prompt.
  static const double _needs = 58;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return LayoutBuilder(
      builder: (context, constraints) {
        if (small || constraints.maxHeight < _needs) {
          return const SizedBox.shrink();
        }
        return Center(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              spacing: OcSpace.s8,
              children: [
                Text(
                  '$name is live',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.bodyStrong.copyWith(color: colors.text),
                ),
                OcButton.primary(
                  label: 'Watch',
                  height: 32,
                  pressedScale: 0.97,
                  onPressed: onWatch,
                ),
              ],
            ),
          ),
        );
      },
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
  const _LivePill();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: OcSpace.s6, vertical: 1),
      decoration: BoxDecoration(
        color: colors.accent,
        borderRadius: BorderRadius.circular(5),
      ),
      child: Text(
        'LIVE',
        style: OcText.label.copyWith(
          fontSize: 10.5,
          letterSpacing: 0.4,
          color: colors.onAccent,
        ),
      ),
    );
  }
}

/// A camera on with no video to draw yet: a head and shoulders, which pop in
/// (§4.2).
class _CameraSilhouette extends StatefulWidget {
  const _CameraSilhouette();

  @override
  State<_CameraSilhouette> createState() => _CameraSilhouetteState();
}

class _CameraSilhouetteState extends State<_CameraSilhouette> {
  bool _shown = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) setState(() => _shown = true);
    });
  }

  @override
  Widget build(BuildContext context) => SpringValue.listenable(
    target: _shown ? 1 : 0,
    spring: OcSpring.pop,
    builder: (context, shown, _) =>
        CustomPaint(painter: _SilhouettePainter(context.oc, shown)),
  );
}

class _SilhouettePainter extends CustomPainter {
  _SilhouettePainter(this.colors, this.shown) : super(repaint: shown);

  /// From the prototype's `cam-in`.
  static const _startScale = 0.9;

  final OcColors colors;
  final ValueListenable<double> shown;

  @override
  void paint(Canvas canvas, Size size) {
    final value = shown.value;
    final paint = Paint()
      ..color = colors.avatarShades[1].withValues(
        alpha: value.clamp(0, 1).toDouble(),
      );
    final scale = _startScale + (1 - _startScale) * value;
    final head = size.width * 0.22;
    final bodyWidth = size.width * 0.44;
    final bodyHeight = size.height * 0.32;
    final corner = Radius.elliptical(bodyWidth * 0.4, bodyHeight * 0.4);
    final body = Rect.fromLTWH(
      (size.width - bodyWidth) / 2,
      size.height + 4 - bodyHeight,
      bodyWidth,
      bodyHeight,
    );
    canvas
      ..drawOval(
        _scaled(
          Rect.fromLTWH(
            (size.width - head) / 2,
            size.height * 0.22,
            head,
            head,
          ),
          scale,
        ),
        paint,
      )
      ..drawRRect(
        RRect.fromRectAndCorners(
          _scaled(body, scale),
          topLeft: corner,
          topRight: corner,
        ),
        paint,
      );
  }

  static Rect _scaled(Rect rect, double scale) => Rect.fromCenter(
    center: rect.center,
    width: rect.width * scale,
    height: rect.height * scale,
  );

  @override
  bool shouldRepaint(_SilhouettePainter oldDelegate) =>
      oldDelegate.colors != colors || oldDelegate.shown != shown;
}

/// A stand-in for a shared screen with no video to draw: lines of text.
class ScreenFeed extends StatelessWidget {
  const ScreenFeed({super.key});

  @override
  Widget build(BuildContext context) =>
      CustomPaint(painter: _ScreenPainter(context.oc));
}

class _ScreenPainter extends CustomPainter {
  _ScreenPainter(this.colors);

  // From the prototype's `.screen` and `.sl`.
  static const _top = 48.0, _side = 0.08, _gap = 12.0;
  static const _titleHeight = 18.0, _titleWidth = 0.42, _titleBelow = 8.0;
  static const _lineHeight = 10.0, _radius = Radius.circular(5);
  static const _lines = [0.86, 0.74, 0.91, 0.58, 0.8];

  final OcColors colors;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = colors.sidebar);
    final left = size.width * _side;
    final width = size.width - 2 * left;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(left, _top, width * _titleWidth, _titleHeight),
        _radius,
      ),
      Paint()..color = colors.avatarShades[2],
    );
    final line = Paint()..color = colors.selected;
    var top = _top + _titleHeight + _titleBelow + _gap;
    for (final share in _lines) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromLTWH(left, top, width * share, _lineHeight),
          _radius,
        ),
        line,
      );
      top += _lineHeight + _gap;
    }
  }

  @override
  bool shouldRepaint(_ScreenPainter oldDelegate) =>
      oldDelegate.colors != colors;
}
