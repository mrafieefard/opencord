import 'package:flutter/material.dart';

import '../view.dart';

/// `select` (design §12.2): dropdown selector of options within the plugin area.
final selectBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
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
        Icon(
          Icons.arrow_drop_down,
          size: 18,
          color: theme.colors['text_secondary']!,
        ),
      ],
    ),
  );

  if (disabled) {
    trigger = Opacity(opacity: 0.5, child: trigger);
  }

  return Material(
    type: MaterialType.transparency,
    child: PopupMenuButton<Object?>(
      enabled: !disabled,
      initialValue: currentValue,
      color: theme.colors['elevated']!,
      elevation: 4,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: theme.colors['border']!, width: 1),
      ),
      onSelected: (val) {
        if (b.node.handles('change')) {
          b.send('change', {'value': val});
        }
      },
      itemBuilder: (context) => [
        for (final opt in options)
          PopupMenuItem<Object?>(
            value: opt['value'],
            child: Text(
              opt['label'] as String? ?? opt['value']?.toString() ?? '',
              style: TextStyle(
                fontFamily: theme.text.fontFamily ?? 'Inter',
                fontSize: 14,
                color: opt['value'] == currentValue
                    ? theme.colors['accent']!
                    : theme.colors['text']!,
              ),
            ),
          ),
      ],
      child: trigger,
    ),
  );
}
