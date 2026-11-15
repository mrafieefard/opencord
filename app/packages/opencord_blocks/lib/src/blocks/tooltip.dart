import 'package:flutter/material.dart' show Tooltip;
import 'package:flutter/widgets.dart';

import '../view.dart';

/// `tooltip` (design §12.2): hover tooltip over child.
final tooltipBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final children = b.node.children;
  if (children.isEmpty) return const SizedBox.shrink();

  final message =
      b.props['text'] as String? ?? b.props['message'] as String? ?? '';
  final child = BlockWidget(id: children.first);

  if (message.isEmpty) return child;
  return Tooltip(message: message, child: child);
}
