import 'package:flutter/material.dart';

import '../theme.dart';
import '../view.dart';
import 'popup_menu.dart';

/// `menu` (design §12.2): trigger child opens a list of items below it inside the area.
final menuBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _MenuWidget(b: b);
}

class _MenuWidget extends StatefulWidget {
  const _MenuWidget({required this.b});

  final BlockBuild b;

  @override
  State<_MenuWidget> createState() => _MenuWidgetState();
}

class _MenuWidgetState extends State<_MenuWidget> {
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
    final theme = b.theme;
    final children = b.node.children;
    if (children.isEmpty) return const SizedBox.shrink();

    final triggerChild = BlockWidget(id: children.first);

    final rawItems = props['items'];
    final items = <Map<String, Object?>>[
      if (rawItems is List)
        for (final item in rawItems)
          if (item is Map) item.cast<String, Object?>(),
    ];

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
              for (final opt in items)
                _MenuItemTile(
                  opt: opt,
                  theme: theme,
                  hostIcon: (name) => b.host.icon(name),
                  onTap: () {
                    close();
                    _entry = null;
                    if (b.node.handles('select')) {
                      b.send('select', {'value': opt['value']});
                    }
                  },
                ),
            ],
          );
        },
        child: triggerChild,
      ),
    );
  }
}

class _MenuItemTile extends StatefulWidget {
  const _MenuItemTile({
    required this.opt,
    required this.theme,
    required this.hostIcon,
    required this.onTap,
  });

  final Map<String, Object?> opt;
  final BlocksTheme theme;
  final IconData? Function(String name) hostIcon;
  final VoidCallback onTap;

  @override
  State<_MenuItemTile> createState() => _MenuItemTileState();
}

class _MenuItemTileState extends State<_MenuItemTile> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = widget.theme;
    final opt = widget.opt;
    final label = opt['label'] as String? ?? opt['value']?.toString() ?? '';
    final isDanger = opt['danger'] == true;
    final iconName = opt['icon'] as String?;
    final iconData = iconName != null ? widget.hostIcon(iconName) : null;

    final bg = _hovered ? theme.colors['hover']! : Colors.transparent;

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
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (iconData != null) ...[
                Icon(
                  iconData,
                  size: 16,
                  color: isDanger
                      ? theme.colors['danger']!
                      : theme.colors['text_secondary']!,
                ),
                const SizedBox(width: 8),
              ],
              Text(
                label,
                style: TextStyle(
                  fontFamily: theme.text.fontFamily ?? 'Inter',
                  fontSize: 14,
                  color: isDanger
                      ? theme.colors['danger']!
                      : theme.colors['text']!,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
