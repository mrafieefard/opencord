import 'package:flutter/material.dart';

import '../theme.dart';
import '../view.dart';
import 'popup_menu.dart';

/// `select` (design §12.2): dropdown selector of options within the plugin area.
final selectBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _SelectWidget(b: b);
}

class _SelectWidget extends StatefulWidget {
  const _SelectWidget({required this.b});

  final BlockBuild b;

  @override
  State<_SelectWidget> createState() => _SelectWidgetState();
}

class _SelectWidgetState extends State<_SelectWidget> {
  OverlayEntry? _entry;

  @override
  void dispose() {
    _closePopup();
    super.dispose();
  }

  void _closePopup() {
    if (_entry != null) {
      if (_entry!.mounted) {
        _entry!.remove();
      }
      _entry = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    final b = widget.b;
    final props = b.props;
    final currentValue = props['value'];
    final placeholder = props['placeholder'] as String? ?? 'Select...';
    final disabled = props['disabled'] == true;
    final theme = b.theme;

    final optionsList = props['options'];
    final options = <Map<String, Object?>>[
      if (optionsList is List)
        for (final item in optionsList)
          if (item is Map) item.cast<String, Object?>(),
    ];

    String? selectedLabel;
    for (final opt in options) {
      if (opt['value'] == currentValue) {
        selectedLabel = opt['label'] as String? ?? opt['value']?.toString();
        break;
      }
    }

    final display = selectedLabel ?? placeholder;

    Widget trigger = Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: theme.colors['surface']!,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: theme.colors['border']!, width: 1),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Flexible(
            child: Text(
              display,
              style: TextStyle(
                fontFamily: theme.text.fontFamily ?? 'Inter',
                fontSize: 14,
                color: selectedLabel != null
                    ? theme.colors['text']!
                    : theme.colors['text_muted']!,
              ),
              overflow: TextOverflow.ellipsis,
            ),
          ),
          const SizedBox(width: 8),
          _arrowDownIcon(b),
        ],
      ),
    );

    if (disabled) {
      return Opacity(opacity: 0.5, child: trigger);
    }

    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () {
          _closePopup();
          _entry = showBlocksPopup(
            context: context,
            theme: theme,
            itemsBuilder: (close) => [
              for (final opt in options)
                _OptionTile(
                  opt: opt,
                  isSelected: opt['value'] == currentValue,
                  theme: theme,
                  onTap: () {
                    close();
                    _entry = null;
                    if (b.node.handles('change')) {
                      b.send('change', {'value': opt['value']});
                    }
                  },
                ),
            ],
          );
        },
        child: trigger,
      ),
    );
  }
}

class _OptionTile extends StatefulWidget {
  const _OptionTile({
    required this.opt,
    required this.isSelected,
    required this.theme,
    required this.onTap,
  });

  final Map<String, Object?> opt;
  final bool isSelected;
  final BlocksTheme theme;
  final VoidCallback onTap;

  @override
  State<_OptionTile> createState() => _OptionTileState();
}

class _OptionTileState extends State<_OptionTile> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = widget.theme;
    final label =
        widget.opt['label'] as String? ?? widget.opt['value']?.toString() ?? '';
    final bg = _hovered
        ? theme.colors['hover']!
        : (widget.isSelected
              ? theme.colors['selected'] ?? Colors.transparent
              : Colors.transparent);

    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.onTap,
        child: Container(
          color: bg,
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Text(
            label,
            style: TextStyle(
              fontFamily: theme.text.fontFamily ?? 'Inter',
              fontSize: 14,
              color: widget.isSelected
                  ? theme.colors['accent']!
                  : theme.colors['text']!,
            ),
          ),
        ),
      ),
    );
  }
}

Widget _arrowDownIcon(BlockBuild b) {
  final icon = b.host.icon('arrow_down');
  if (icon != null) {
    return Icon(icon, size: 18, color: b.theme.colors['text_secondary']!);
  }
  return CustomPaint(
    size: const Size(10, 6),
    painter: _ArrowDownPainter(b.theme.colors['text_secondary']!),
  );
}

class _ArrowDownPainter extends CustomPainter {
  _ArrowDownPainter(this.color);
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
      ..moveTo(0, 0)
      ..lineTo(size.width / 2, size.height)
      ..lineTo(size.width, 0);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_ArrowDownPainter oldDelegate) =>
      oldDelegate.color != color;
}
