import 'package:flutter/material.dart';

import '../theme.dart';
import '../view.dart';
import 'touch.dart';

/// `tabs` (design §12.2): horizontal tab bar with active indicator.
final tabsBlock = BlockRenderer(_tabs);

/// `segmented` (design §12.2): segmented pill control.
final segmentedBlock = BlockRenderer(_segmented);

Widget _tabs(BlockBuild b) {
  final props = b.props;
  final currentValue = props['value'];
  final theme = b.theme;
  final options = _parseOptions(props['options']);

  return SingleChildScrollView(
    scrollDirection: Axis.horizontal,
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final opt in options) ...[
          _tabItem(
            b: b,
            opt: opt,
            isSelected: opt['value'] == currentValue,
            theme: theme,
          ),
          const SizedBox(width: 4),
        ],
      ],
    ),
  );
}

Widget _tabItem({
  required BlockBuild b,
  required Map<String, Object?> opt,
  required bool isSelected,
  required BlocksTheme theme,
}) {
  final val = opt['value'];
  final label = opt['label'] as String? ?? val?.toString() ?? '';
  final iconName = opt['icon'] as String?;
  final iconData = iconName != null ? b.host.icon(iconName) : null;

  return MouseRegion(
    cursor: SystemMouseCursors.click,
    child: GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () {
        if (!isSelected && b.node.handles('change')) {
          b.send('change', {'value': val});
        }
      },
      child: touchTarget(
        b.theme,
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(
            border: Border(
              bottom: BorderSide(
                color: isSelected
                    ? theme.colors['accent']!
                    : Colors.transparent,
                width: 2,
              ),
            ),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (iconData != null) ...[
                Icon(
                  iconData,
                  size: 16,
                  color: isSelected
                      ? theme.colors['accent']!
                      : theme.colors['text_secondary']!,
                ),
                const SizedBox(width: 6),
              ],
              Text(
                label,
                style: TextStyle(
                  fontFamily: theme.text.fontFamily ?? 'Inter',
                  fontSize: 14,
                  fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
                  color: isSelected
                      ? theme.colors['accent']!
                      : theme.colors['text_secondary']!,
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

Widget _segmented(BlockBuild b) {
  final props = b.props;
  final currentValue = props['value'];
  final theme = b.theme;
  final options = _parseOptions(props['options']);

  return Container(
    padding: const EdgeInsets.all(2),
    decoration: BoxDecoration(
      color: theme.colors['surface']!,
      borderRadius: BorderRadius.circular(8),
      border: Border.all(color: theme.colors['border']!, width: 1),
    ),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final opt in options)
          _segmentItem(
            b: b,
            opt: opt,
            isSelected: opt['value'] == currentValue,
            theme: theme,
          ),
      ],
    ),
  );
}

Widget _segmentItem({
  required BlockBuild b,
  required Map<String, Object?> opt,
  required bool isSelected,
  required BlocksTheme theme,
}) {
  final val = opt['value'];
  final label = opt['label'] as String? ?? val?.toString() ?? '';
  final iconName = opt['icon'] as String?;
  final iconData = iconName != null ? b.host.icon(iconName) : null;

  return MouseRegion(
    cursor: SystemMouseCursors.click,
    child: GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () {
        if (!isSelected && b.node.handles('change')) {
          b.send('change', {'value': val});
        }
      },
      child: touchTarget(
        b.theme,
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
          decoration: BoxDecoration(
            color: isSelected ? theme.colors['selected']! : Colors.transparent,
            borderRadius: BorderRadius.circular(6),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (iconData != null) ...[
                Icon(
                  iconData,
                  size: 14,
                  color: isSelected
                      ? theme.colors['text']!
                      : theme.colors['text_secondary']!,
                ),
                const SizedBox(width: 6),
              ],
              Text(
                label,
                style: TextStyle(
                  fontFamily: theme.text.fontFamily ?? 'Inter',
                  fontSize: 13,
                  fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
                  color: isSelected
                      ? theme.colors['text']!
                      : theme.colors['text_secondary']!,
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

List<Map<String, Object?>> _parseOptions(Object? raw) {
  if (raw is! List) return const [];
  return [
    for (final item in raw)
      if (item is Map) item.cast<String, Object?>(),
  ];
}
