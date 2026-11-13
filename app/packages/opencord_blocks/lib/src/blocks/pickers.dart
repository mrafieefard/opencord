import 'package:flutter/material.dart';

import '../view.dart';

/// `image_picker` (design §12.2, §13.3): button to pick and upload an image.
final imagePickerBlock = BlockRenderer((b) => _picker(b, isImage: true));

/// `file_picker` (design §12.2, §13.3): button to pick and upload a file.
final filePickerBlock = BlockRenderer((b) => _picker(b, isImage: false));

Widget _picker(BlockBuild b, {required bool isImage}) {
  return _PickerWidget(
    key: ValueKey<int>(b.node.id),
    build: b,
    isImage: isImage,
  );
}

class _PickerWidget extends StatefulWidget {
  const _PickerWidget({super.key, required this.build, required this.isImage});

  final BlockBuild build;
  final bool isImage;

  @override
  State<_PickerWidget> createState() => _PickerWidgetState();
}

class _PickerWidgetState extends State<_PickerWidget> {
  bool _uploading = false;

  Future<void> _pick() async {
    if (_uploading) return;
    final b = widget.build;
    final props = b.props;
    final acceptRaw = props['accept'];
    final accept = <String>[
      if (acceptRaw is List)
        for (final item in acceptRaw)
          if (item is String) item,
    ];

    try {
      final path = await b.host.pickFile(image: widget.isImage, accept: accept);
      if (path == null) return;

      if (mounted) setState(() => _uploading = true);
      await b.host.upload(b.node.id, path);
    } on Exception catch (error, stack) {
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: error,
          stack: stack,
          library: 'opencord_blocks',
          context: ErrorDescription(
            'while picking or uploading file from picker',
          ),
        ),
      );
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final b = widget.build;
    final props = b.props;
    final text =
        props['text'] as String? ??
        (widget.isImage ? 'Choose Image' : 'Choose File');
    final theme = b.theme;

    Widget child;
    if (_uploading) {
      child = SizedBox.square(
        dimension: 16,
        child: CircularProgressIndicator(
          strokeWidth: 2,
          color: theme.colors['accent']!,
        ),
      );
    } else {
      child = Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _pickerIcon(b, widget.isImage),
          const SizedBox(width: 8),
          Text(
            text,
            style: TextStyle(
              fontFamily: theme.text.fontFamily ?? 'Inter',
              fontSize: 14,
              fontWeight: FontWeight.w500,
              color: theme.colors['text']!,
            ),
          ),
        ],
      );
    }

    return MouseRegion(
      cursor: _uploading ? MouseCursor.defer : SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: _uploading ? null : _pick,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          decoration: BoxDecoration(
            color: theme.colors['surface']!,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: theme.colors['border']!, width: 1),
          ),
          child: child,
        ),
      ),
    );
  }
}

Widget _pickerIcon(BlockBuild b, bool isImage) {
  final name = isImage ? 'image' : 'upload';
  final icon = b.host.icon(name);
  if (icon != null) {
    return Icon(icon, size: 16, color: b.theme.colors['text']!);
  }
  return CustomPaint(
    size: const Size(16, 16),
    painter: isImage
        ? _ImageIconPainter(b.theme.colors['text']!)
        : _UploadIconPainter(b.theme.colors['text']!),
  );
}

class _ImageIconPainter extends CustomPainter {
  _ImageIconPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.5
      ..style = PaintingStyle.stroke;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(1, 1, size.width - 2, size.height - 2),
        const Radius.circular(2),
      ),
      paint,
    );
    final path = Path()
      ..moveTo(3, size.height - 4)
      ..lineTo(7, 7)
      ..lineTo(10, 11)
      ..lineTo(13, size.height - 4);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_ImageIconPainter oldDelegate) =>
      oldDelegate.color != color;
}

class _UploadIconPainter extends CustomPainter {
  _UploadIconPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.5
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..style = PaintingStyle.stroke;
    final path = Path()
      ..moveTo(size.width / 2, size.height - 3)
      ..lineTo(size.width / 2, 3)
      ..moveTo(3, 8)
      ..lineTo(size.width / 2, 3)
      ..lineTo(size.width - 3, 8);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_UploadIconPainter oldDelegate) =>
      oldDelegate.color != color;
}
