import 'package:flutter/material.dart';

import '../view.dart';

/// `menu` (design §12.2): trigger child opens a list of options below it inside the area.
final menuBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final props = b.props;
  final theme = b.theme;
  final children = b.node.children;
  if (children.isEmpty) return const SizedBox.shrink();

  final triggerChild = BlockWidget(id: children.first);

  final optionsList = props['options'];
  final options = <Map<String, Object?>>[
    if (optionsList is List)
      for (final item in optionsList)
        if (item is Map) item.cast<String, Object?>(),
  ];

  return Material(
    type: MaterialType.transparency,
    child: PopupMenuButton<Object?>(
      color: theme.colors['elevated']!,
      elevation: 4,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: theme.colors['border']!, width: 1),
      ),
      onSelected: (val) {
        if (b.node.handles('select')) {
          b.send('select', {'value': val});
        }
      },
      itemBuilder: (context) => [
        for (final opt in options)
          PopupMenuItem<Object?>(
            value: opt['value'],
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (opt['icon'] case final String iconName) ...[
                  if (b.host.icon(iconName) case final iconData?)
                    Icon(
                      iconData,
                      size: 16,
                      color: opt['danger'] == true
                          ? theme.colors['danger']!
                          : theme.colors['text_secondary']!,
                    ),
                  const SizedBox(width: 8),
                ],
                Text(
                  opt['label'] as String? ?? opt['value']?.toString() ?? '',
                  style: TextStyle(
                    fontFamily: theme.text.fontFamily ?? 'Inter',
                    fontSize: 14,
                    color: opt['danger'] == true
                        ? theme.colors['danger']!
                        : theme.colors['text']!,
                  ),
                ),
              ],
            ),
          ),
      ],
      child: triggerChild,
    ),
  );
}
