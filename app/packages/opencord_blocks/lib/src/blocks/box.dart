import 'package:flutter/widgets.dart';

import '../view.dart';

/// `box` (design §12.2): one child with padding, background, border,
/// radius and shadow; pressable and hoverable when the plugin handles it.
/// Only the events the plugin handles are listened for, so a `double_press`
/// handler alone delays a plain press by the double-tap wait.
final boxBlock = BlockRenderer(_content, around: _presses);

Widget _content(BlockBuild b) {
  final children = b.node.children;
  if (children.isEmpty) return const SizedBox.shrink();
  return BlockWidget(key: ValueKey<int>(children.first), id: children.first);
}

const _pressEvents = ['press', 'double_press', 'long_press', 'secondary_press'];

Widget _presses(BlockBuild b, Widget drawn) {
  final node = b.node;
  bool has(String event) => node.handles(event);
  final pressable = _pressEvents.any(has);
  var shown = drawn;
  if (pressable) {
    shown = GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: has('press') ? () => b.send('press') : null,
      onDoubleTap: has('double_press') ? () => b.send('double_press') : null,
      onLongPress: has('long_press') ? () => b.send('long_press') : null,
      onSecondaryTapUp: has('secondary_press')
          ? (details) => b.send('secondary_press', {
              'x': details.localPosition.dx,
              'y': details.localPosition.dy,
            })
          : null,
      child: shown,
    );
  }
  if (pressable || has('hover_enter') || has('hover_exit')) {
    shown = MouseRegion(
      cursor: pressable ? SystemMouseCursors.click : MouseCursor.defer,
      onEnter: has('hover_enter') ? (_) => b.send('hover_enter') : null,
      onExit: has('hover_exit') ? (_) => b.send('hover_exit') : null,
      child: shown,
    );
  }
  return shown;
}
