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

    final path = await b.host.pickFile(image: widget.isImage, accept: accept);
    if (path == null) return;

    if (mounted) setState(() => _uploading = true);
    try {
      await b.host.upload(b.node.id, path);
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
          Icon(
            widget.isImage ? Icons.image_outlined : Icons.attach_file,
            size: 16,
            color: theme.colors['text']!,
          ),
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
