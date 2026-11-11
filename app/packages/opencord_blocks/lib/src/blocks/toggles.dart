import 'package:flutter/material.dart';

import '../theme.dart';
import '../view.dart';

/// `checkbox` (design §12.2): square check toggle with optional label.
final checkboxBlock = BlockRenderer(_checkbox);

/// `switch` (design §12.2): pill toggle switch with optional label.
final switchBlock = BlockRenderer(_switch);

/// `radio` (design §12.2): circular radio button with optional label.
final radioBlock = BlockRenderer(_radio);

Widget _checkbox(BlockBuild b) {
  final props = b.props;
  final value = props['value'] == true;
  final text = props['text'] as String?;
  final disabled = props['disabled'] == true;
  final theme = b.theme;
  final interactive = !disabled;

  final box = Container(
    width: 18,
    height: 18,
    decoration: BoxDecoration(
      color: value ? theme.colors['accent']! : theme.colors['surface']!,
      borderRadius: BorderRadius.circular(4),
      border: Border.all(
        color: value ? theme.colors['accent']! : theme.colors['border']!,
        width: 1.5,
      ),
    ),
    alignment: Alignment.center,
    child: value ? _checkIcon(b, theme) : null,
  );

  return _wrapToggle(
    b: b,
    control: box,
    text: text,
    disabled: disabled,
    interactive: interactive,
    onTap: () => b.send('change', {'value': !value}),
  );
}

Widget _switch(BlockBuild b) {
  final props = b.props;
  final value = props['value'] == true;
  final text = props['text'] as String?;
  final disabled = props['disabled'] == true;
  final theme = b.theme;
  final interactive = !disabled;

  final track = AnimatedContainer(
    duration: const Duration(milliseconds: 150),
    width: 38,
    height: 22,
    padding: const EdgeInsets.all(2),
    decoration: BoxDecoration(
      color: value ? theme.colors['accent']! : theme.colors['selected']!,
      borderRadius: BorderRadius.circular(11),
      border: Border.all(color: theme.colors['border']!, width: 1),
    ),
    child: AnimatedAlign(
      duration: const Duration(milliseconds: 150),
      alignment: value ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        width: 16,
        height: 16,
        decoration: BoxDecoration(
          color: value
              ? theme.colors['on_accent']!
              : theme.colors['text_secondary']!,
          shape: BoxShape.circle,
        ),
      ),
    ),
  );

  return _wrapToggle(
    b: b,
    control: track,
    text: text,
    disabled: disabled,
    interactive: interactive,
    onTap: () => b.send('change', {'value': !value}),
  );
}

Widget _radio(BlockBuild b) {
  final props = b.props;
  final value = props['value'] == true;
  final text = props['text'] as String?;
  final disabled = props['disabled'] == true;
  final theme = b.theme;
  final interactive = !disabled;

  final circle = Container(
    width: 18,
    height: 18,
    decoration: BoxDecoration(
      shape: BoxShape.circle,
      border: Border.all(
        color: value ? theme.colors['accent']! : theme.colors['border']!,
        width: 2,
      ),
    ),
    alignment: Alignment.center,
    child: value
        ? Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: theme.colors['accent']!,
            ),
          )
        : null,
  );

  return _wrapToggle(
    b: b,
    control: circle,
    text: text,
    disabled: disabled,
    interactive: interactive,
    onTap: () {
      if (!value) b.send('change', {'value': true});
    },
  );
}

Widget _wrapToggle({
  required BlockBuild b,
  required Widget control,
  required String? text,
  required bool disabled,
  required bool interactive,
  required VoidCallback onTap,
}) {
  Widget row = control;
  if (text != null && text.isNotEmpty) {
    row = Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        control,
        const SizedBox(width: 8),
        Flexible(
          child: Text(
            text,
            style: TextStyle(
              fontFamily: b.theme.text.fontFamily ?? 'Inter',
              fontSize: 14,
              color: b.theme.colors['text']!,
            ),
          ),
        ),
      ],
    );
  }

  if (disabled) {
    row = Opacity(opacity: 0.5, child: row);
  }

  return MouseRegion(
    cursor: interactive ? SystemMouseCursors.click : MouseCursor.defer,
    child: GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: interactive && b.node.handles('change') ? onTap : null,
      child: row,
    ),
  );
}

Widget _checkIcon(BlockBuild b, BlocksTheme theme) {
  final icon = b.host.icon('check');
  if (icon != null) {
    return Icon(icon, size: 14, color: theme.colors['on_accent']!);
  }
  return CustomPaint(
    size: const Size(14, 14),
    painter: _CheckPainter(theme.colors['on_accent']!),
  );
}

class _CheckPainter extends CustomPainter {
  _CheckPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..style = PaintingStyle.stroke;
    final path = Path()
      ..moveTo(size.width * 0.22, size.height * 0.52)
      ..lineTo(size.width * 0.44, size.height * 0.74)
      ..lineTo(size.width * 0.78, size.height * 0.28);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_CheckPainter oldDelegate) => oldDelegate.color != color;
}
