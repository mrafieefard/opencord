import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

class _GoldenHost extends FakeHost {
  @override
  IconData? icon(String name) => switch (name) {
    'star' => const IconData(0xf09a, fontFamily: 'MaterialSymbolsRounded'),
    'bolt' => const IconData(0xea0b, fontFamily: 'MaterialSymbolsRounded'),
    'chat' => const IconData(0xe0c9, fontFamily: 'MaterialSymbolsRounded'),
    _ => null,
  };
}

List<Map<String, Object?>> _listAnimPage() {
  final items = <Map<String, Object?>>[];
  for (var i = 0; i < 5; i++) {
    final itemId = 100 + i;
    final rowId = 200 + i;
    final badgeId = 300 + i;
    final textId = 400 + i;

    items.addAll([
      create(
        badgeId,
        'badge',
        props: {
          'text': i == 0 ? 'VIP' : '#$i',
          'tone': i == 0 ? 'accent' : 'surface',
        },
      ),
      create(
        textId,
        'text',
        props: {
          'text': 'Channel member $i',
          'weight': 'medium',
          'color': 'text',
        },
      ),
      create(
        rowId,
        'row',
        props: {
          'gap': 12,
          'padding': 8,
          'cross': 'center',
          'background': 'surface',
          'radius': 8,
        },
        children: [badgeId, textId],
      ),
      create(
        itemId,
        'box',
        key: '#$i',
        props: {'height': 48},
        children: [rowId],
      ),
    ]);
  }

  return [
    ...items,
    // Header
    create(
      11,
      'text',
      props: {
        'text': 'Members List & Animations',
        'size': 18,
        'weight': 'bold',
        'color': 'text',
      },
    ),
    // Animated card
    create(
      21,
      'text',
      props: {
        'text': 'Live Channel Activity',
        'weight': 'semibold',
        'color': 'on_accent',
      },
    ),
    create(22, 'badge', props: {'text': 'Active', 'tone': 'success'}),
    create(
      20,
      'row',
      props: {'gap': 12, 'cross': 'center'},
      children: [21, 22],
    ),
    create(
      2,
      'box',
      props: {
        'padding': 16,
        'background': 'accent',
        'radius': 12,
        'shadow': 'sm',
        'animate': {'duration': 200, 'curve': 'ease_out'},
      },
      children: [20],
    ),

    // Lazy list container
    create(
      3,
      'list',
      props: {'count': 10, 'item_height': 52, 'gap': 8},
      children: [for (var i = 0; i < 5; i++) 100 + i],
    ),

    create(
      30,
      'box',
      props: {
        'height': 300,
        'padding': 12,
        'background': 'surface',
        'border': {'width': 1, 'color': 'border'},
        'radius': 12,
      },
      children: [3],
    ),

    // Main column
    create(
      1,
      'column',
      props: {
        'gap': 16,
        'padding': 16,
        'width': 'fill',
        'background': 'background',
      },
      children: [11, 2, 30],
    ),
    roots(1),
  ];
}

void main() {
  for (final (name, theme) in themes) {
    for (final width in [800.0, 360.0]) {
      testWidgets('list_anim_${name}_${width.toInt()}', (tester) async {
        final host = _GoldenHost();
        final controller = BlocksController();
        addTearDown(controller.dispose);
        controller.applyBatch(batch(1, _listAnimPage()));

        await withRealShadows(() async {
          await pumpView(
            tester,
            controller,
            host: host,
            theme: theme,
            width: width,
            height: 600,
            window: Size(width, 600),
          );
          await tester.pump();
          await expectLater(
            find.byType(BlocksView),
            matchesGoldenFile('goldens/list_anim_${name}_${width.toInt()}.png'),
          );
        });
      });
    }
  }
}
