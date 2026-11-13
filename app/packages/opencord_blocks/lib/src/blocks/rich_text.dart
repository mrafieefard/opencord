import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `rich_text` (design §12.2): markdown formatted content (bold, italic, code,
/// links, mentions, emoji) drawn through [BlocksHost.richText].
final richTextBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final props = b.props;
  final text = props['text'] as String? ?? '';
  final size = numberProp(props['size']) ?? b.theme.text.fontSize ?? 14.0;
  final color = b.theme.resolveColor(props['color']) ?? b.theme.colors['text']!;
  final style = b.theme.text.copyWith(fontSize: size, color: color);

  return b.host.richText(text, style);
}
