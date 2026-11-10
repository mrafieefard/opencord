import 'package:flutter/material.dart';

import '../view.dart';

/// `button` (design §12.2): primary, secondary, danger and plain buttons;
/// shows a spinner when [loading] and dims when [disabled].
final buttonBlock = BlockRenderer(_button);

/// `icon_button` (design §12.2): compact button holding an icon.
final iconButtonBlock = BlockRenderer(_iconButton);

Widget _button(BlockBuild b) {
  final props = b.props;
  final text = props['text'] as String? ?? '';
  final variant = props['variant'] as String? ?? 'primary';
  final iconName = props['icon'] as String?;
  final loading = props['loading'] == true;
  final disabled = props['disabled'] == true;
  final theme = b.theme;

  final (Color bg, Color fg, Border? border) = switch (variant) {
    'secondary' => (
      theme.colors['surface']!,
      theme.colors['text']!,
      Border.all(color: theme.colors['border']!, width: 1),
    ),
    'danger' => (theme.colors['danger']!, theme.colors['on_accent']!, null),
    'plain' => (Colors.transparent, theme.colors['text']!, null),
    _ => (theme.colors['accent']!, theme.colors['on_accent']!, null),
  };

  final iconData = iconName != null ? b.host.icon(iconName) : null;
  final interactive = !disabled && !loading;

  final children = <Widget>[];
  if (loading) {
    children.add(
      SizedBox.square(
        dimension: 16,
        child: CircularProgressIndicator(strokeWidth: 2, color: fg),
      ),
    );
  } else if (iconData != null) {
    children.add(Icon(iconData, size: 16, color: fg));
  }
  if (text.isNotEmpty) {
    if (children.isNotEmpty) children.add(const SizedBox(width: 8));
    children.add(
      Text(
        text,
        style: TextStyle(
          fontFamily: theme.text.fontFamily ?? 'Inter',
          fontSize: 14,
          fontWeight: FontWeight.w600,
          color: fg,
        ),
      ),
    );
  }
  final child = Row(
    mainAxisSize: MainAxisSize.min,
    mainAxisAlignment: MainAxisAlignment.center,
    children: children,
  );

  Widget button = Container(
    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
    decoration: BoxDecoration(
      color: bg,
      borderRadius: BorderRadius.circular(8),
      border: border,
    ),
    alignment: Alignment.center,
    child: child,
  );

  if (disabled) {
    button = Opacity(opacity: 0.5, child: button);
  }

  return MouseRegion(
    cursor: interactive ? SystemMouseCursors.click : MouseCursor.defer,
    child: GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: interactive && b.node.handles('press')
          ? () => b.send('press')
          : null,
      onLongPress: interactive && b.node.handles('long_press')
          ? () => b.send('long_press')
          : null,
      child: button,
    ),
  );
}

Widget _iconButton(BlockBuild b) {
  final props = b.props;
  final iconName = props['icon'] as String? ?? '';
  final disabled = props['disabled'] == true;
  final theme = b.theme;
  final iconData = b.host.icon(iconName);
  final interactive = !disabled;

  Widget child = SizedBox.square(
    dimension: 32,
    child: Center(
      child: iconData != null
          ? Icon(iconData, size: 18, color: theme.colors['text']!)
          : const SizedBox.shrink(),
    ),
  );

  if (disabled) {
    child = Opacity(opacity: 0.5, child: child);
  }

  return MouseRegion(
    cursor: interactive ? SystemMouseCursors.click : MouseCursor.defer,
    child: GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: interactive && b.node.handles('press')
          ? () => b.send('press')
          : null,
      child: child,
    ),
  );
}
