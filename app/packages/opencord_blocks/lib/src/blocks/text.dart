import 'package:flutter/material.dart' show SelectableText;
import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../host.dart';
import '../view.dart';

/// `text` (design §12.2): draws plain text with weights mapping to Inter's
/// 400, 500, 600, 700; custom fonts load through [BlocksHost.font].
final textBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _TextBlockWidget(key: ValueKey<int>(b.node.id), build: b);
}

class _TextBlockWidget extends StatefulWidget {
  const _TextBlockWidget({super.key, required this.build});

  final BlockBuild build;

  @override
  State<_TextBlockWidget> createState() => _TextBlockWidgetState();
}

class _TextBlockWidgetState extends State<_TextBlockWidget> {
  @override
  void initState() {
    super.initState();
    _checkFont();
  }

  @override
  void didUpdateWidget(_TextBlockWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.build.props['font'] != widget.build.props['font']) {
      _checkFont();
    }
  }

  void _checkFont() {
    final font = widget.build.props['font'] as String?;
    if (font == null || font.isEmpty) return;
    final controller = widget.build.controller;
    if (controller.resolvedFont(font) != null) return;

    controller.loadFont(font, widget.build.host).then((family) {
      if (mounted && family != null) setState(() {});
    });
  }

  @override
  Widget build(BuildContext context) {
    final b = widget.build;
    final props = b.props;
    final text = props['text'] as String? ?? '';
    if (text.isEmpty) return const SizedBox.shrink();

    final font = props['font'] as String?;
    final fontFamily =
        (font != null ? b.controller.resolvedFont(font) : null) ??
        (b.theme.text.fontFamily ?? 'Inter');

    final size = numberProp(props['size']) ?? b.theme.text.fontSize ?? 14.0;
    final weight = _weight(props['weight'] as String?);
    final italic = props['italic'] == true;
    final color =
        b.theme.resolveColor(props['color']) ?? b.theme.colors['text']!;
    final align = _align(props['align'] as String?);
    final maxLines = props['max_lines'] is num
        ? (props['max_lines'] as num).toInt()
        : null;
    final selectable = props['selectable'] == true;
    final direction = textDirectionOf(text);

    final style = TextStyle(
      fontFamily: fontFamily,
      fontSize: size,
      fontWeight: weight,
      fontStyle: italic ? FontStyle.italic : FontStyle.normal,
      color: color,
    );

    if (selectable) {
      return SelectableText(
        text,
        style: style,
        textAlign: align,
        textDirection: direction,
        maxLines: maxLines,
      );
    }

    return Text(
      text,
      style: style,
      textAlign: align,
      textDirection: direction,
      maxLines: maxLines,
      overflow: maxLines != null ? TextOverflow.ellipsis : TextOverflow.clip,
      softWrap: true,
    );
  }
}

FontWeight _weight(String? weight) => switch (weight) {
  'bold' => FontWeight.w700,
  'semibold' => FontWeight.w600,
  'medium' => FontWeight.w500,
  _ => FontWeight.w400,
};

TextAlign _align(String? align) => switch (align) {
  'center' => TextAlign.center,
  'end' => TextAlign.end,
  _ => TextAlign.start,
};

bool _isRtlRune(int rune) =>
    (rune >= 0x0590 && rune <= 0x08FF) ||
    (rune >= 0xFB1D && rune <= 0xFDFF) ||
    (rune >= 0xFE70 && rune <= 0xFEFF) ||
    (rune >= 0x10800 && rune <= 0x10FFF) ||
    (rune >= 0x1E800 && rune <= 0x1EFFF);

final _letter = RegExp(r'\p{L}', unicode: true);

/// Determines text direction by the first directional letter.
TextDirection? textDirectionOf(String text) {
  for (final rune in text.runes) {
    if (_isRtlRune(rune)) return TextDirection.rtl;
    if (_letter.hasMatch(String.fromCharCode(rune))) return TextDirection.ltr;
  }
  return null;
}
