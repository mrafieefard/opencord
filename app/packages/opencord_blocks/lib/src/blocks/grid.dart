import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `grid` (design §12.2): children placed in a grid of `columns` or
/// `min_column_width` with a `gap` between them.
final gridBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final props = b.props;
  final gap = numberProp(props['gap']) ?? 0;
  final columnsProp = props['columns'] is num
      ? (props['columns'] as num).toInt()
      : null;
  final minColumnWidth = numberProp(props['min_column_width']);
  final controller = b.controller;
  final ids = b.node.children;

  return ListenableBuilder(
    listenable: Listenable.merge([for (final id in ids) controller.watch(id)]),
    builder: (context, _) {
      final visibleIds = <int>[];
      for (final id in ids) {
        final child = controller.node(id);
        if (child == null || child.props['visible'] == false) continue;
        visibleIds.add(id);
      }
      if (visibleIds.isEmpty) return const SizedBox.shrink();

      return LayoutBuilder(
        builder: (context, constraints) {
          int columns = 1;
          if (columnsProp != null && columnsProp > 0) {
            columns = columnsProp;
          } else if (minColumnWidth != null && minColumnWidth > 0) {
            final availableWidth = constraints.hasBoundedWidth
                ? constraints.maxWidth
                : MediaQuery.sizeOf(context).width;
            columns = math.max(
              1,
              ((availableWidth + gap) / (minColumnWidth + gap)).floor(),
            );
          }

          final rows = <Widget>[];
          for (var i = 0; i < visibleIds.length; i += columns) {
            final rowChildren = <Widget>[];
            final end = math.min(i + columns, visibleIds.length);
            for (var j = i; j < end; j++) {
              if (j > i && gap > 0) {
                rowChildren.add(SizedBox(width: gap));
              }
              final childId = visibleIds[j];
              rowChildren.add(
                Expanded(
                  key: ValueKey<int>(childId),
                  child: BlockWidget(id: childId),
                ),
              );
            }
            // Fill remaining columns in the last row to keep column widths consistent.
            final count = end - i;
            for (var k = count; k < columns; k++) {
              if (rowChildren.isNotEmpty && gap > 0) {
                rowChildren.add(SizedBox(width: gap));
              }
              rowChildren.add(const Expanded(child: SizedBox.shrink()));
            }

            if (rows.isNotEmpty && gap > 0) {
              rows.add(SizedBox(height: gap));
            }
            rows.add(
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: rowChildren,
              ),
            );
          }

          return Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: rows,
          );
        },
      );
    },
  );
}
