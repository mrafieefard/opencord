import 'dart:convert';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/rendering.dart';

import '../common_props.dart';
import '../host.dart';
import '../theme.dart';

/// One drawing item of a canvas (design §12.6), read once per item object
/// the core sent. Every item has `type`, `id`, and may set `stroke`
/// (`{color, width}`), `fill` (a colour or a gradient), `opacity` (0–1) and
/// `transform` (`{translate = {x, y}, rotate (degrees), scale}`, about the
/// centre of the item's bounds).
sealed class CanvasItem {
  const CanvasItem(this.look);
  final ItemLook look;
}

/// `line`: `x1, y1, x2, y2`.
final class LineItem extends CanvasItem {
  const LineItem(super.look, this.from, this.to);
  final Offset from;
  final Offset to;
}

/// `path`: `points` flat (`[x, y, x, y, …]`), `closed`.
final class PathItem extends CanvasItem {
  PathItem(super.look, List<Offset> points, this.closed)
    : path = _path(points, closed),
      bounds = _pointBounds(points);
  final ui.Path path;
  final bool closed;
  final Rect bounds;
}

/// `rect`: `x, y, w, h`, `radius`.
final class RectItem extends CanvasItem {
  const RectItem(super.look, this.rect, this.radius);
  final Rect rect;
  final double radius;
}

/// `ellipse`: `x, y, w, h`.
final class EllipseItem extends CanvasItem {
  const EllipseItem(super.look, this.rect);
  final Rect rect;
}

/// `text`: `x, y` (its top left), `text`, `size`, `color`.
final class TextItem extends CanvasItem {
  const TextItem(super.look, this.at, this.text, this.size, this.color);
  final Offset at;
  final String text;
  final double? size;
  final Object? color;
}

/// `image`: `x, y, w, h` and one image source, stretched to fill the rect.
final class ImageItem extends CanvasItem {
  ImageItem(super.look, this.rect, this.source) : key = jsonEncode(source);
  final Rect rect;
  final Map<String, Object?> source;

  /// The source as one string: items naming the same image share it.
  final String key;
}

/// What an item looks like besides its shape.
@immutable
class ItemLook {
  const ItemLook({this.fill, this.stroke, this.opacity = 1, this.transform});

  /// A colour or a gradient, resolved with the theme when drawn.
  final Object? fill;
  final ItemStroke? stroke;
  final double opacity;
  final ItemTransform? transform;
}

/// `{color, width}`: a stroke, or the ink a canvas draws with. Without a
/// colour it's the theme's `text`; without a width, 1.
@immutable
class ItemStroke {
  const ItemStroke({this.color, this.width = 1});
  final Object? color;
  final double width;
}

@immutable
class ItemTransform {
  const ItemTransform({
    this.translate = Offset.zero,
    this.radians = 0,
    this.scale = 1,
  });

  final Offset translate;
  final double radians;
  final double scale;

  /// Moves, then turns and scales about [origin], as CSS's `transform`
  /// with its origin at the item's centre.
  void apply(Canvas canvas, Offset origin) {
    canvas.translate(origin.dx + translate.dx, origin.dy + translate.dy);
    if (radians != 0) canvas.rotate(radians);
    if (scale != 1) canvas.scale(scale);
    canvas.translate(-origin.dx, -origin.dy);
  }
}

/// An item the renderer can't draw. Its message names the field, never the
/// item's content.
class CanvasItemError implements Exception {
  CanvasItemError(this.message);
  final String message;

  @override
  String toString() => 'Canvas item: $message';
}

/// Reads [items] into drawable ones, reusing what [read] holds for an item
/// object read before. An item that can't be drawn is left out and its
/// problem goes to [onProblem], with its place in the list.
List<CanvasItem> readItems(
  List<Map<String, Object?>> items,
  BlocksTheme theme,
  Expando<CanvasItem> read,
  void Function(String problem) onProblem,
) {
  final drawable = <CanvasItem>[];
  for (final (index, item) in items.indexed) {
    final known = read[item];
    if (known != null) {
      drawable.add(known);
      continue;
    }
    try {
      final fresh = _item(item, theme);
      read[item] = fresh;
      drawable.add(fresh);
    } on CanvasItemError catch (error) {
      onProblem('items[$index]: ${error.message}');
    }
  }
  return List.unmodifiable(drawable);
}

/// Reads an `ink` or `stroke` prop; null when it's absent.
ItemStroke? readStroke(Object? value, BlocksTheme theme, String name) {
  if (value == null) return null;
  if (value is! Map<String, Object?>) {
    throw CanvasItemError('$name must be {color, width}');
  }
  final color = value['color'];
  if (color != null) _colour(color, theme, '$name.color', gradient: true);
  final width = _number(value, 'width', name: '$name.width') ?? 1;
  if (width < 0) throw CanvasItemError('$name.width must be 0 or more');
  return ItemStroke(color: color, width: width);
}

CanvasItem _item(Map<String, Object?> item, BlocksTheme theme) {
  final look = ItemLook(
    fill: switch (item['fill']) {
      null => null,
      final fill => _colour(fill, theme, 'fill', gradient: true),
    },
    stroke: readStroke(item['stroke'], theme, 'stroke'),
    opacity: (_number(item, 'opacity') ?? 1).clamp(0, 1).toDouble(),
    transform: _transform(item['transform']),
  );
  Rect box() {
    final corner = Offset(_need(item, 'x'), _need(item, 'y'));
    // A negative size grows up or left from the corner.
    return Rect.fromPoints(
      corner,
      corner.translate(_need(item, 'w'), _need(item, 'h')),
    );
  }

  return switch (item['type']) {
    'line' => LineItem(
      look,
      Offset(_need(item, 'x1'), _need(item, 'y1')),
      Offset(_need(item, 'x2'), _need(item, 'y2')),
    ),
    'path' => PathItem(look, _points(item['points']), item['closed'] == true),
    'rect' => RectItem(look, box(), math.max(0, _number(item, 'radius') ?? 0)),
    'ellipse' => EllipseItem(look, box()),
    'text' => TextItem(
      look,
      Offset(_need(item, 'x'), _need(item, 'y')),
      switch (item['text']) {
        final String text => text,
        _ => throw CanvasItemError('text needs its text'),
      },
      _number(item, 'size'),
      switch (item['color']) {
        null => null,
        final color => _colour(color, theme, 'color', gradient: false),
      },
    ),
    'image' => ImageItem(
      look,
      box(),
      imageSource(item) ??
          (throw CanvasItemError(
            'image needs exactly one of ${imageSourceNames.join(', ')}',
          )),
    ),
    null => throw CanvasItemError('needs a type'),
    _ => throw CanvasItemError('unknown type'),
  };
}

double? _number(Map<String, Object?> item, String field, {String? name}) {
  final value = item[field];
  if (value == null) return null;
  if (value is num && value.isFinite) return value.toDouble();
  throw CanvasItemError('${name ?? field} must be a finite number');
}

double _need(Map<String, Object?> item, String field) =>
    _number(item, field) ?? (throw CanvasItemError('needs $field'));

Object _colour(
  Object value,
  BlocksTheme theme,
  String name, {
  required bool gradient,
}) {
  if (theme.resolveColor(value) != null) return value;
  if (gradient && theme.resolveGradient(value) != null) return value;
  throw CanvasItemError(
    '$name must be a colour name, "#rrggbb", "#rrggbbaa"'
    '${gradient ? ' or a gradient {from, to, angle?}' : ''}',
  );
}

ItemTransform? _transform(Object? value) {
  if (value == null) return null;
  if (value is! Map<String, Object?>) {
    throw CanvasItemError('transform must be {translate, rotate, scale}');
  }
  var translate = Offset.zero;
  if (value['translate'] case final Object moved) {
    if (moved is! Map<String, Object?>) {
      throw CanvasItemError('transform.translate must be {x, y}');
    }
    translate = Offset(
      _number(moved, 'x', name: 'transform.translate.x') ?? 0,
      _number(moved, 'y', name: 'transform.translate.y') ?? 0,
    );
  }
  final degrees = _number(value, 'rotate', name: 'transform.rotate') ?? 0;
  return ItemTransform(
    translate: translate,
    radians: degrees * math.pi / 180,
    scale: _number(value, 'scale', name: 'transform.scale') ?? 1,
  );
}

List<Offset> _points(Object? value) {
  if (value is! List<Object?> || value.length.isOdd) {
    throw CanvasItemError('points must be a flat list of x, y pairs');
  }
  return [
    for (var i = 0; i < value.length; i += 2)
      switch ((value[i], value[i + 1])) {
        (final num x, final num y) when x.isFinite && y.isFinite => Offset(
          x.toDouble(),
          y.toDouble(),
        ),
        _ => throw CanvasItemError('points must be finite numbers'),
      },
  ];
}

ui.Path _path(List<Offset> points, bool closed) {
  final path = ui.Path();
  if (points.isEmpty) return path;
  path.moveTo(points.first.dx, points.first.dy);
  for (final point in points.skip(1)) {
    path.lineTo(point.dx, point.dy);
  }
  if (closed) path.close();
  return path;
}

Rect _pointBounds(List<Offset> points) {
  if (points.isEmpty) return Rect.zero;
  var left = points.first.dx, right = left;
  var top = points.first.dy, bottom = top;
  for (final point in points) {
    left = math.min(left, point.dx);
    right = math.max(right, point.dx);
    top = math.min(top, point.dy);
    bottom = math.max(bottom, point.dy);
  }
  return Rect.fromLTRB(left, top, right, bottom);
}

/// The images a canvas's items name, loaded through the host and kept while
/// an item names them. Tells its listeners (the items' painter) when one
/// loads, or an animated one moves to its next frame.
///
/// When entries leave: when no item names them any more ([keep]), and on
/// [dispose].
class CanvasImages extends ChangeNotifier {
  CanvasImages(this._onError);

  final void Function(Object error, StackTrace stack) _onError;
  final _images = <String, _CanvasImage>{};
  var _disposed = false;

  /// The loaded image for an item's [ImageItem.key], or null until then.
  ui.Image? operator [](String key) => _images[key]?.info?.image;

  /// Loads what [items] name and isn't loaded yet; lets go of the rest.
  void keep(
    List<CanvasItem> items,
    BlocksHost host,
    ImageConfiguration configuration,
  ) {
    final wanted = {
      for (final item in items)
        if (item is ImageItem) item.key: item.source,
    };
    _images.removeWhere((key, image) {
      if (wanted.containsKey(key)) return false;
      image.release();
      return true;
    });
    for (final MapEntry(:key, value: source) in wanted.entries) {
      if (_images.containsKey(key)) continue;
      final image = _CanvasImage();
      _images[key] = image;
      host
          .image(source)
          .then<void>(
            (provider) {
              if (provider == null || image.released || _disposed) return;
              image.listen(
                provider.resolve(configuration),
                ImageStreamListener(
                  (info, _) {
                    if (image.released || _disposed) {
                      info.dispose();
                      return;
                    }
                    image.info?.dispose();
                    image.info = info;
                    notifyListeners();
                  },
                  onError: (Object error, StackTrace? stack) =>
                      _onError(error, stack ?? StackTrace.current),
                ),
              );
            },
            onError: (Object error, StackTrace stack) {
              if (!image.released && !_disposed) _onError(error, stack);
            },
          );
    }
  }

  @override
  void dispose() {
    _disposed = true;
    for (final image in _images.values) {
      image.release();
    }
    _images.clear();
    super.dispose();
  }
}

class _CanvasImage {
  ImageStream? _stream;
  ImageStreamListener? _listener;
  ImageInfo? info;
  var released = false;

  void listen(ImageStream stream, ImageStreamListener listener) {
    _stream = stream;
    _listener = listener;
    stream.addListener(listener);
  }

  void release() {
    released = true;
    if (_listener case final listener?) _stream?.removeListener(listener);
    _stream = null;
    _listener = null;
    info?.dispose();
    info = null;
  }
}

/// Text laid out for one theme.
typedef LaidText = ({BlocksTheme theme, TextPainter painter});

/// Draws a canvas's items in order, inside its bounds. Repaints when its
/// items or theme change (a `draw` op, a new theme) or an image loads.
class ItemsPainter extends CustomPainter {
  ItemsPainter({
    required this.items,
    required this.theme,
    required this.images,
    required this.texts,
  }) : super(repaint: images);

  final List<CanvasItem> items;
  final BlocksTheme theme;
  final CanvasImages images;

  /// Text items laid out before, kept with the item they belong to.
  final Expando<LaidText> texts;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.clipRect(Offset.zero & size);
    for (final item in items) {
      final look = item.look;
      if (look.opacity == 0) continue;
      final transform = look.transform;
      if (transform != null) {
        canvas.save();
        transform.apply(canvas, _bounds(item).center);
      }
      _paint(canvas, item);
      if (transform != null) canvas.restore();
    }
  }

  Rect _bounds(CanvasItem item) => switch (item) {
    LineItem(:final from, :final to) => Rect.fromPoints(from, to),
    PathItem(:final bounds) => bounds,
    RectItem(:final rect) || EllipseItem(:final rect) => rect,
    ImageItem(:final rect) => rect,
    TextItem(:final at) => at & _text(item).painter.size,
  };

  void _paint(Canvas canvas, CanvasItem item) {
    switch (item) {
      case LineItem(:final from, :final to):
        final stroke = item.look.stroke ?? const ItemStroke();
        if (stroke.width > 0) {
          canvas.drawLine(from, to, _stroke(stroke, _bounds(item), item.look));
        }
      case PathItem(:final path, :final closed, :final bounds):
        _shape(
          item.look,
          bounds,
          closed,
          (paint) => canvas.drawPath(path, paint),
        );
      case RectItem(:final rect, :final radius):
        _shape(
          item.look,
          rect,
          true,
          radius > 0
              ? (paint) => canvas.drawRRect(
                  RRect.fromRectAndRadius(rect, Radius.circular(radius)),
                  paint,
                )
              : (paint) => canvas.drawRect(rect, paint),
        );
      case EllipseItem(:final rect):
        _shape(item.look, rect, true, (paint) => canvas.drawOval(rect, paint));
      case TextItem(:final at):
        _text(item).painter.paint(canvas, at);
      case ImageItem(:final rect, :final key):
        final image = images[key];
        if (image == null) return;
        canvas.drawImageRect(
          image,
          Offset.zero & Size(image.width.toDouble(), image.height.toDouble()),
          rect,
          Paint()
            ..filterQuality = FilterQuality.medium
            ..color = Color.fromRGBO(0, 0, 0, item.look.opacity),
        );
    }
  }

  /// A shape's fill, then its stroke. With neither, a closed shape is
  /// filled with the theme's `text`, and an open one stroked with it.
  void _shape(
    ItemLook look,
    Rect bounds,
    bool closed,
    void Function(Paint paint) draw,
  ) {
    final fill = look.fill;
    final stroke = look.stroke;
    if (fill == null && stroke == null) {
      draw(
        closed
            ? _fill('text', bounds, look.opacity)
            : _stroke(const ItemStroke(), bounds, look),
      );
      return;
    }
    if (fill != null) draw(_fill(fill, bounds, look.opacity));
    if (stroke != null && stroke.width > 0) {
      draw(_stroke(stroke, bounds, look));
    }
  }

  Paint _fill(Object colour, Rect bounds, double opacity) {
    final paint = Paint()..isAntiAlias = true;
    if (theme.resolveGradient(colour) case final gradient?) {
      return paint..shader = gradient.scale(opacity).createShader(bounds);
    }
    final color = theme.resolveColor(colour) ?? theme.colors['text']!;
    return paint..color = color.withValues(alpha: color.a * opacity);
  }

  Paint _stroke(ItemStroke stroke, Rect bounds, ItemLook look) =>
      _fill(
          stroke.color ?? 'text',
          bounds.inflate(stroke.width / 2),
          look.opacity,
        )
        ..style = PaintingStyle.stroke
        ..strokeWidth = stroke.width
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round;

  LaidText _text(TextItem item) {
    final laid = texts[item];
    if (laid != null && identical(laid.theme, theme)) return laid;
    laid?.painter.dispose();
    final color = theme.resolveColor(item.color ?? 'text')!;
    final painter = TextPainter(
      text: TextSpan(
        text: item.text,
        style: theme.text.copyWith(
          fontSize: item.size,
          color: color.withValues(alpha: color.a * item.look.opacity),
        ),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    final fresh = (theme: theme, painter: painter);
    texts[item] = fresh;
    return fresh;
  }

  @override
  bool shouldRepaint(ItemsPainter oldDelegate) =>
      !identical(oldDelegate.items, items) ||
      !identical(oldDelegate.theme, theme);
}
