import 'dart:math' as math;

import 'package:flutter/physics.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

/// Paints its child moving from where it was to where it is, on OcSpring.move.
/// Arrives on OcSpring.tile when it has no earlier rectangle.
///
/// It moves only when [generation] changes, so a window resize or a pane
/// opening just puts it in its new place. Layout is final at once: only what's
/// painted moves. It measures in `paint`, so its first frame is already in the
/// old place. Under reduced motion it never moves.
class FlipBox extends SingleChildRenderObjectWidget {
  const FlipBox({
    super.key,
    required this.generation,
    this.arriveDelay = Duration.zero,
    super.child,
  });

  /// [arriveDelay] for the [index]th of a group arriving together (§5.14).
  static Duration stagger(int index) =>
      Duration(milliseconds: math.min(index * _staggerStep, _staggerMost));

  static const _staggerStep = 16;
  static const _staggerMost = 80;

  /// What the box's place depends on, which moving is for: not its size or its
  /// place on screen.
  final Object generation;

  /// How long it waits, unseen, before arriving.
  final Duration arriveDelay;

  @override
  RenderFlipBox createRenderObject(BuildContext context) => RenderFlipBox(
    generation: generation,
    arriveDelay: arriveDelay,
    reduced: MediaQuery.disableAnimationsOf(context),
  );

  @override
  void updateRenderObject(BuildContext context, RenderFlipBox renderObject) {
    renderObject
      ..newGeneration(generation)
      ..arriveDelay = arriveDelay
      ..reduced = MediaQuery.disableAnimationsOf(context);
  }
}

/// What's inside a [FlipBox]'s frame: it fades back in after a big resize, so
/// it's never seen stretched (§5.14). Outside a [FlipBox] it does nothing.
class FlipBoxContents extends SingleChildRenderObjectWidget {
  const FlipBoxContents({super.key, super.child});

  @override
  RenderFlipBoxContents createRenderObject(BuildContext context) =>
      RenderFlipBoxContents();
}

/// One run of a [RenderFlipBox]'s offset and scale from the top left, each
/// heading for no offset and a scale of 1.
class _Flight {
  _Flight(
    OcSpring spring,
    this.start,
    this.delay, {
    required double dx,
    required double dy,
    required double scaleX,
    required double scaleY,
    double dxSpeed = 0,
    double dySpeed = 0,
    double scaleXSpeed = 0,
    double scaleYSpeed = 0,
  }) : _dx = spring.simulation(dx, 0, dxSpeed),
       _dy = spring.simulation(dy, 0, dySpeed),
       _scaleX = spring.simulation(scaleX, 1, scaleXSpeed),
       _scaleY = spring.simulation(scaleY, 1, scaleYSpeed);

  /// A channel with no way left to go but some speed never reads as done.
  static const _longest = 2.0;

  final Duration start;
  final Duration delay;
  final SpringSimulation _dx;
  final SpringSimulation _dy;
  final SpringSimulation _scaleX;
  final SpringSimulation _scaleY;

  double _seconds(Duration now) => math.max(
    0,
    (now - start - delay).inMicroseconds / Duration.microsecondsPerSecond,
  );

  double dx(Duration now) => _dx.x(_seconds(now));
  double dy(Duration now) => _dy.x(_seconds(now));
  double scaleX(Duration now) => _scaleX.x(_seconds(now));
  double scaleY(Duration now) => _scaleY.x(_seconds(now));

  double _speed(SpringSimulation channel, Duration now) =>
      now - start < delay ? 0 : channel.dx(_seconds(now));
  double dxSpeed(Duration now) => _speed(_dx, now);
  double dySpeed(Duration now) => _speed(_dy, now);
  double scaleXSpeed(Duration now) => _speed(_scaleX, now);
  double scaleYSpeed(Duration now) => _speed(_scaleY, now);

  bool isDone(Duration now) {
    final time = _seconds(now);
    return time >= _longest ||
        (_dx.isDone(time) &&
            _dy.isDone(time) &&
            _scaleX.isDone(time) &&
            _scaleY.isDone(time));
  }
}

class RenderFlipBox extends RenderProxyBox {
  RenderFlipBox({
    required this.generation,
    required this.arriveDelay,
    required this.reduced,
  });

  // From the prototype's playTiles(), as §5.14 gives them.
  static const _arriveRise = 6.0;
  static const _arriveScale = 0.97;
  static const _arriveFade = Duration(milliseconds: 120);
  static const _contentsFadeDelay = Duration(milliseconds: 70);
  static const _contentsFade = Duration(milliseconds: 140);

  /// A change of scale this big would show what's inside stretched.
  static const _bigScale = 0.05;

  /// Less than this from its place isn't worth moving.
  static const _tinyOffset = 1.0;
  static const _tinyScale = 0.01;

  Duration arriveDelay;
  bool reduced;

  Object generation;

  /// Set from the widget: moving is what a new [generation] is for.
  void newGeneration(Object value) {
    if (value == generation) return;
    generation = value;
    _generationChanged = true;
    markNeedsPaint();
  }

  bool _generationChanged = false;

  /// Where layout last put it on screen, without [_transform].
  Rect? _place;
  _Flight? _flight;

  /// When the arrival's fade starts.
  Duration? _fadingIn;

  /// When the last big resize happened.
  Duration? _resized;

  Matrix4 _transform = Matrix4.identity();
  double _opacity = 1;
  double _contentsOpacity = 1;
  RenderFlipBoxContents? _contents;

  late final Ticker _ticker = Ticker((_) {
    markNeedsPaint();
    _contents?.markNeedsPaint();
  }, debugLabel: 'FlipBox');

  /// The box itself, on its way in. Its [_transform] is what `localToGlobal`
  /// on its child reports.
  double get opacity => _opacity;

  // The opacity it pushes is a layer, which what clips it has to know.
  @override
  bool get alwaysNeedsCompositing => child != null;

  @override
  void detach() {
    _ticker.stop();
    super.detach();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  void _takeOff(Rect place, Duration now) {
    final before = _place;
    final changed = _generationChanged;
    _place = place;
    _generationChanged = false;
    if (reduced) {
      _flight = null;
      _fadingIn = null;
      _resized = null;
      return;
    }
    if (place.isEmpty) return;
    if (before == null) {
      _arrive(place, now);
    } else if (changed && !before.isEmpty) {
      _move(before, place, now);
    }
  }

  void _arrive(Rect place, Duration now) {
    final shrunk = 1 - _arriveScale;
    _flight = _Flight(
      OcSpring.tile,
      now,
      arriveDelay,
      // Scaled about its centre, as seen from the top left.
      dx: place.width * shrunk / 2,
      dy: _arriveRise + place.height * shrunk / 2,
      scaleX: _arriveScale,
      scaleY: _arriveScale,
    );
    _fadingIn = now + arriveDelay;
  }

  /// From where it's painted now, at the speed it has, to [place].
  void _move(Rect before, Rect place, Duration now) {
    final flight = _flight;
    final left = before.left + (flight?.dx(now) ?? 0);
    final top = before.top + (flight?.dy(now) ?? 0);
    final width = before.width * (flight?.scaleX(now) ?? 1);
    final height = before.height * (flight?.scaleY(now) ?? 1);
    final dx = left - place.left;
    final dy = top - place.top;
    final scaleX = width / place.width;
    final scaleY = height / place.height;
    if (flight == null &&
        dx.abs() < _tinyOffset &&
        dy.abs() < _tinyOffset &&
        (scaleX - 1).abs() < _tinyScale &&
        (scaleY - 1).abs() < _tinyScale) {
      return;
    }
    _flight = _Flight(
      OcSpring.move,
      now,
      Duration.zero,
      dx: dx,
      dy: dy,
      scaleX: scaleX,
      scaleY: scaleY,
      dxSpeed: flight?.dxSpeed(now) ?? 0,
      dySpeed: flight?.dySpeed(now) ?? 0,
      scaleXSpeed: (flight?.scaleXSpeed(now) ?? 0) * before.width / place.width,
      scaleYSpeed:
          (flight?.scaleYSpeed(now) ?? 0) * before.height / place.height,
    );
    if ((scaleX - 1).abs() > _bigScale || (scaleY - 1).abs() > _bigScale) {
      _resized = now;
    }
  }

  void _advance(Duration now) {
    final flight = _flight;
    if (flight != null && flight.isDone(now)) _flight = null;
    final moving = _flight;
    _transform = moving == null
        ? Matrix4.identity()
        : (Matrix4.diagonal3Values(moving.scaleX(now), moving.scaleY(now), 1)
            ..setTranslationRaw(moving.dx(now), moving.dy(now), 0));

    if (_fadingIn case final start?) {
      _opacity = _fade(now - start, Duration.zero, _arriveFade);
      if (_opacity == 1) _fadingIn = null;
    } else {
      _opacity = 1;
    }
    if (_resized case final start?) {
      _contentsOpacity = _fade(now - start, _contentsFadeDelay, _contentsFade);
      if (_contentsOpacity == 1) _resized = null;
    } else {
      _contentsOpacity = 1;
    }

    final busy = _flight != null || _fadingIn != null || _resized != null;
    if (busy && !_ticker.isActive) {
      _ticker.start();
    } else if (!busy && _ticker.isActive) {
      _ticker.stop();
    }
  }

  static double _fade(Duration time, Duration delay, Duration length) {
    final faded = time - delay;
    if (faded <= Duration.zero) return 0;
    if (faded >= length) return 1;
    return OcMotion.curve.transform(
      faded.inMicroseconds / length.inMicroseconds,
    );
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    final now = SchedulerBinding.instance.currentFrameTimeStamp;
    _takeOff(localToGlobal(Offset.zero) & size, now);
    _advance(now);
    if (child == null) return;
    final alpha = (_opacity * 255).round();
    if (alpha == 0) return;
    if (alpha == 255) {
      _paintMoved(context, offset);
    } else {
      context.pushOpacity(offset, alpha, _paintMoved);
    }
  }

  void _paintMoved(PaintingContext context, Offset offset) {
    if (_flight == null) {
      super.paint(context, offset);
    } else {
      context.pushTransform(needsCompositing, offset, _transform, super.paint);
    }
  }

  @override
  void applyPaintTransform(RenderBox child, Matrix4 transform) {
    transform.multiply(_transform);
  }

  // Where it's painted, which may not be inside the box.
  @override
  bool hitTest(BoxHitTestResult result, {required Offset position}) =>
      hitTestChildren(result, position: position);

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      result.addWithPaintTransform(
        transform: _transform,
        position: position,
        hitTest: (result, position) =>
            super.hitTestChildren(result, position: position),
      );
}

class RenderFlipBoxContents extends RenderProxyBox {
  RenderFlipBox? _box;

  double get opacity => _box?._contentsOpacity ?? 1;

  // The opacity it pushes is a layer, which what clips it has to know.
  @override
  bool get alwaysNeedsCompositing => child != null;

  @override
  void attach(PipelineOwner owner) {
    super.attach(owner);
    var ancestor = parent;
    while (ancestor != null && ancestor is! RenderFlipBox) {
      ancestor = ancestor.parent;
    }
    final box = ancestor as RenderFlipBox?;
    box?._contents = this;
    _box = box;
  }

  @override
  void detach() {
    _box?._contents = null;
    _box = null;
    super.detach();
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    if (child == null) return;
    final alpha = (opacity * 255).round();
    if (alpha == 0) return;
    if (alpha == 255) {
      super.paint(context, offset);
    } else {
      context.pushOpacity(offset, alpha, super.paint);
    }
  }
}
