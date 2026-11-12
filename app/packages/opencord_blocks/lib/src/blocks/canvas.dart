import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import '../node.dart';
import '../view.dart';
import 'canvas_items.dart';

/// `canvas` (design §12.6): the plugin's drawing items, drawn in order by
/// one painter that repaints only on a `draw` op or an image load; pointer,
/// wheel, pan and scale events for the handlers the plugin has; and ink,
/// strokes the renderer draws under the pointer itself.
///
/// It fills the room it's given; where that room has no end (a column's
/// height), it's 300 × 150, as an HTML canvas is.
final canvasBlock = BlockRenderer((b) => _Canvas(node: b.node, scope: b.scope));

/// The most points one ink stroke holds before it ends and a new one goes
/// on from its last point: a path's limit
/// (`opencord_plugins::limits::CANVAS_MAX_PATH_POINTS`), so a plugin can keep
/// each stroke as one `path` item.
const maxStrokePoints = 10000;

class _Canvas extends StatefulWidget {
  const _Canvas({required this.node, required this.scope});

  final BlockNode node;
  final BlocksScope scope;

  @override
  State<_Canvas> createState() => _CanvasState();
}

class _CanvasState extends State<_Canvas> {
  /// `pointer_move` goes at most 60 times a second per pointer; a vsync's
  /// jitter may bring one up to half an interval early.
  static const _moveEvery = Duration(microseconds: 16667);
  static const _moveSlack = Duration(microseconds: 8333);

  /// Ink points go to the plugin every 50 ms, or as soon as 64 wait.
  static const _strokeEvery = Duration(milliseconds: 50);
  static const _strokeBatch = 64;

  /// A finished stroke stays drawn until an item with its id is set, or this
  /// long.
  static const _strokeKept = Duration(seconds: 2);

  static final _random = math.Random();

  final _read = Expando<CanvasItem>();
  final _texts = Expando<LaidText>();
  late final _images = CanvasImages(
    (error, stack) => _report(error, stack, 'loading a canvas image'),
  );
  List<Map<String, Object?>>? _readFrom;
  Object? _readWith;
  List<CanvasItem> _items = const [];
  var _reportedItem = false;

  /// Moves waiting for their pointer's next turn. When entries leave: the
  /// pointer's up or cancel, a send that leaves nothing waiting once its
  /// interval has passed, and [dispose].
  final _moves = <int, _MoveGate>{};
  int? _moveFrame;

  /// Strokes being drawn, by pointer. When entries leave: the pointer's up
  /// or cancel, and [dispose].
  final _inking = <int, _InkStroke>{};

  /// Every stroke drawn here, in order. When entries leave: an item with
  /// the stroke's id set after it ended, [_strokeKept] after it ended, and
  /// [dispose].
  final _strokes = <_InkStroke>[];
  final _inkMoved = _Repaint();

  Offset _focal = Offset.zero;
  double _scale = 1;

  BlockNode get _node => widget.node;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(_Canvas oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.node.items, _node.items)) {
      _confirmStrokes(oldWidget.node.items, _node.items);
    }
    _sync();
  }

  @override
  void dispose() {
    if (_moveFrame case final frame?) {
      SchedulerBinding.instance.cancelFrameCallbackWithId(frame);
    }
    _moves.clear();
    for (final stroke in _strokes) {
      stroke.batch?.cancel();
      stroke.expiry?.cancel();
    }
    _strokes.clear();
    _inking.clear();
    _images.dispose();
    _inkMoved.dispose();
    super.dispose();
  }

  /// Reads the items again when a `draw` op or the theme changed them, and
  /// loads the images they name.
  void _sync() {
    final theme = widget.scope.theme;
    if (identical(_readFrom, _node.items) && identical(_readWith, theme)) {
      return;
    }
    _readFrom = _node.items;
    _readWith = theme;
    _items = readItems(_node.items, theme, _read, (problem) {
      if (_reportedItem) return;
      _reportedItem = true;
      _report(CanvasItemError(problem), StackTrace.current, 'reading items');
    });
    _images.keep(
      _items,
      widget.scope.host,
      createLocalImageConfiguration(context),
    );
  }

  /// Tells the app's log once what went wrong, never the item's content.
  void _report(Object error, StackTrace stack, String what) {
    FlutterError.reportError(
      FlutterErrorDetails(
        exception: error,
        stack: stack,
        library: 'opencord_blocks',
        context: ErrorDescription('while $what for a plugin canvas'),
      ),
    );
  }

  bool _handles(String event) => _node.handles(event);

  void _send(String event, Map<String, Object?> data) =>
      widget.scope.controller.event(_node.id, event, data);

  // Pointers.

  void _down(PointerDownEvent event) {
    _send('pointer_down', _pointer(event));
    _startStroke(event);
  }

  void _move(PointerEvent event) {
    _queueMove(event);
    final stroke = _inking[event.pointer];
    if (stroke != null) _addPoint(event.pointer, stroke, event);
  }

  void _end(PointerEvent event, String name) {
    if (_moves.remove(event.pointer)?.waiting case final waiting?) {
      _send('pointer_move', waiting);
    }
    _endStroke(event.pointer);
    _send(name, _pointer(event));
  }

  /// Keeps the pointer's newest move until its next turn, at the next vsync.
  void _queueMove(PointerEvent event) {
    if (!_handles('pointer_move')) return;
    (_moves[event.pointer] ??= _MoveGate()).waiting = _pointer(event);
    _moveFrame ??= SchedulerBinding.instance.scheduleFrameCallback(_sendMoves);
  }

  void _sendMoves(Duration now) {
    _moveFrame = null;
    var waiting = false;
    _moves.removeWhere((pointer, gate) {
      final data = gate.waiting;
      final next = gate.next;
      if (data != null && (next == null || now >= next - _moveSlack)) {
        _send('pointer_move', data);
        gate.waiting = null;
        gate.next = (next != null && next > now ? next : now) + _moveEvery;
      }
      if (gate.waiting != null) {
        waiting = true;
        return false;
      }
      return gate.next! <= now;
    });
    if (waiting) {
      _moveFrame = SchedulerBinding.instance.scheduleFrameCallback(
        _sendMoves,
        rescheduling: true,
      );
    }
  }

  void _wheel(PointerSignalEvent event) {
    if (event is! PointerScrollEvent || !_handles('wheel')) return;
    // Claimed, so a scrolling block around the canvas doesn't scroll too.
    GestureBinding.instance.pointerSignalResolver.register(event, (event) {
      final scroll = event as PointerScrollEvent;
      _send('wheel', {
        'x': scroll.localPosition.dx,
        'y': scroll.localPosition.dy,
        'dx': scroll.scrollDelta.dx,
        'dy': scroll.scrollDelta.dy,
      });
    });
  }

  // Pan and scale, from one recogniser: Flutter's scale is pan's superset.

  void _gestureStart(ScaleStartDetails details) {
    _focal = details.localFocalPoint;
    _scale = 1;
    _gesture('start', Offset.zero);
  }

  void _gestureUpdate(ScaleUpdateDetails details) {
    _focal = details.localFocalPoint;
    _scale = details.scale;
    _gesture('update', details.focalPointDelta);
  }

  void _gestureEnd(ScaleEndDetails details) => _gesture('end', Offset.zero);

  void _gesture(String phase, Offset delta) {
    final at = {'x': _focal.dx, 'y': _focal.dy};
    if (_handles('pan')) {
      _send('pan', {...at, 'dx': delta.dx, 'dy': delta.dy, 'phase': phase});
    }
    if (_handles('scale')) {
      _send('scale', {...at, 'scale': _scale, 'phase': phase});
    }
  }

  // Ink.

  ItemStroke? get _ink {
    try {
      return readStroke(_node.props['ink'], widget.scope.theme, 'ink');
    } on CanvasItemError catch (error, stack) {
      // The core checked `ink` as `{color, width}`; a colour it can't name
      // would be a schema the two sides disagree on.
      _report(error, stack, 'reading ink');
      return null;
    }
  }

  void _startStroke(PointerDownEvent event) {
    if (event.kind == PointerDeviceKind.invertedStylus ||
        event.buttons & kPrimaryButton == 0) {
      return;
    }
    final ink = _ink;
    if (ink == null) return;
    final theme = widget.scope.theme;
    // A gradient has no one colour to draw ink with: the theme's text.
    final color =
        theme.resolveColor(ink.color ?? 'text') ?? theme.colors['text']!;
    _begin(event.pointer, color, ink.width, event);
  }

  void _begin(int pointer, Color color, double width, PointerEvent event) {
    final stroke = _InkStroke(
      id: 'ink_${_hex()}${_hex()}',
      color: color,
      width: width,
    );
    _strokes.add(stroke);
    _inking[pointer] = stroke;
    _addPoint(pointer, stroke, event);
  }

  static String _hex() =>
      _random.nextInt(1 << 32).toRadixString(16).padLeft(8, '0');

  void _addPoint(int pointer, _InkStroke stroke, PointerEvent event) {
    final at = event.localPosition;
    stroke.points.add(at);
    stroke.waiting.addAll([at.dx, at.dy, _pressure(event) ?? 0.5]);
    _inkMoved.repaint();
    if (stroke.points.length >= maxStrokePoints) {
      _endStroke(pointer);
      _begin(pointer, stroke.color, stroke.width, event);
    } else if (stroke.waiting.length >= 3 * _strokeBatch) {
      _sendStroke(stroke, done: false);
    } else {
      stroke.batch ??= Timer(
        _strokeEvery,
        () => _sendStroke(stroke, done: false),
      );
    }
  }

  void _sendStroke(_InkStroke stroke, {required bool done}) {
    stroke.batch?.cancel();
    stroke.batch = null;
    final points = stroke.waiting;
    if (points.isEmpty && !done) return;
    stroke.waiting = [];
    _send('stroke', {'stroke': stroke.id, 'points': points, 'done': done});
  }

  void _endStroke(int pointer) {
    final stroke = _inking.remove(pointer);
    if (stroke == null) return;
    _sendStroke(stroke, done: true);
    stroke.expiry = Timer(_strokeKept, () {
      if (!mounted) return;
      _strokes.remove(stroke);
      _inkMoved.repaint();
    });
  }

  /// Lets go of each ended stroke whose id an item was just set to: the
  /// plugin draws it now.
  void _confirmStrokes(
    List<Map<String, Object?>> before,
    List<Map<String, Object?>> after,
  ) {
    if (!_strokes.any((stroke) => stroke.expiry != null)) return;
    final was = {for (final item in before) item['id']: item};
    final now = {for (final item in after) item['id']: item};
    final confirmed = _strokes.where((stroke) {
      if (stroke.expiry == null) return false;
      final item = now[stroke.id];
      return item != null && !identical(item, was[stroke.id]);
    }).toList();
    if (confirmed.isEmpty) return;
    for (final stroke in confirmed) {
      stroke.expiry?.cancel();
      _strokes.remove(stroke);
    }
    _inkMoved.repaint();
  }

  Map<Type, GestureRecognizerFactory> _gestures() {
    if (_handles('pan') || _handles('scale')) {
      return {
        ScaleGestureRecognizer:
            GestureRecognizerFactoryWithHandlers<ScaleGestureRecognizer>(
              ScaleGestureRecognizer.new,
              (recogniser) => recogniser
                ..onStart = _gestureStart
                ..onUpdate = _gestureUpdate
                ..onEnd = _gestureEnd,
            ),
      };
    }
    const pointerEvents = [
      'pointer_down',
      'pointer_move',
      'pointer_up',
      'pointer_cancel',
    ];
    if (_node.props['ink'] != null || pointerEvents.any(_handles)) {
      // Drawing on the canvas never scrolls or presses what's around it.
      return {
        EagerGestureRecognizer:
            GestureRecognizerFactoryWithHandlers<EagerGestureRecognizer>(
              EagerGestureRecognizer.new,
              (_) {},
            ),
      };
    }
    return const {};
  }

  @override
  Widget build(BuildContext context) {
    final scope = widget.scope;
    return RawGestureDetector(
      behavior: HitTestBehavior.opaque,
      gestures: _gestures(),
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: _down,
        onPointerMove: _move,
        onPointerHover: _queueMove,
        onPointerUp: (event) => _end(event, 'pointer_up'),
        onPointerCancel: (event) => _end(event, 'pointer_cancel'),
        onPointerSignal: _wheel,
        child: LimitedBox(
          maxWidth: 300,
          maxHeight: 150,
          child: SizedBox.expand(
            child: Stack(
              fit: StackFit.expand,
              children: [
                // Each layer repaints alone: ink moving never redraws the
                // items, and nothing outside the canvas redraws either.
                RepaintBoundary(
                  child: CustomPaint(
                    painter: ItemsPainter(
                      items: _items,
                      theme: scope.theme,
                      images: _images,
                      texts: _texts,
                    ),
                  ),
                ),
                RepaintBoundary(
                  child: CustomPaint(painter: _InkPainter(_strokes, _inkMoved)),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// `{x, y, pointer, kind, buttons, pressure?}`.
Map<String, Object?> _pointer(PointerEvent event) => {
  'x': event.localPosition.dx,
  'y': event.localPosition.dy,
  'pointer': event.pointer,
  'kind': switch (event.kind) {
    PointerDeviceKind.touch => 'touch',
    PointerDeviceKind.stylus || PointerDeviceKind.invertedStylus => 'pen',
    PointerDeviceKind.mouse ||
    PointerDeviceKind.trackpad ||
    PointerDeviceKind.unknown => 'mouse',
  },
  'buttons': event.buttons,
  'pressure': ?_pressure(event),
};

/// The pressure from 0 to 1, where the device reports one.
double? _pressure(PointerEvent event) {
  final range = event.pressureMax - event.pressureMin;
  if (!(range > 0)) return null;
  return ((event.pressure - event.pressureMin) / range).clamp(0.0, 1.0);
}

class _MoveGate {
  Map<String, Object?>? waiting;

  /// When the pointer's next move may go.
  Duration? next;
}

class _InkStroke {
  _InkStroke({required this.id, required this.color, required this.width});

  final String id;
  final Color color;
  final double width;
  final points = <Offset>[];

  /// Flat `[x, y, pressure, …]` not yet sent.
  var waiting = <double>[];
  Timer? batch;

  /// Set when the stroke ends.
  Timer? expiry;
}

class _Repaint extends ChangeNotifier {
  void repaint() => notifyListeners();
}

/// Draws the ink strokes; repaints on each point, alone.
class _InkPainter extends CustomPainter {
  _InkPainter(this.strokes, Listenable moved) : super(repaint: moved);

  final List<_InkStroke> strokes;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.clipRect(Offset.zero & size);
    for (final stroke in strokes) {
      final paint = Paint()
        ..color = stroke.color
        ..isAntiAlias = true;
      final points = stroke.points;
      if (points.length == 1 || stroke.width == 0) {
        canvas.drawCircle(points.first, math.max(stroke.width, 1) / 2, paint);
        continue;
      }
      final path = Path()..moveTo(points.first.dx, points.first.dy);
      for (final point in points.skip(1)) {
        path.lineTo(point.dx, point.dy);
      }
      canvas.drawPath(
        path,
        paint
          ..style = PaintingStyle.stroke
          ..strokeWidth = stroke.width
          ..strokeCap = StrokeCap.round
          ..strokeJoin = StrokeJoin.round,
      );
    }
  }

  @override
  bool shouldRepaint(_InkPainter oldDelegate) =>
      !identical(oldDelegate.strokes, strokes);
}
