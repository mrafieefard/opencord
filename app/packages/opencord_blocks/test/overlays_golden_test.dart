import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

class _GoldenHost extends FakeHost {
  @override
  IconData? icon(String name) => switch (name) {
    'star' => Icons.star,
    'close' => Icons.close,
    _ => null,
  };
}

List<Map<String, Object?>> _overlaysPage() {
  return [
    // Background content
    create(
      11,
      'text',
      props: {
        'text': 'Plugin Workspace',
        'size': 20,
        'weight': 'bold',
        'color': 'text',
      },
    ),
    create(
      12,
      'text',
      props: {
        'text':
            'Background area stays visible and dimmed while modal is active.',
        'color': 'text_secondary',
      },
    ),
    create(10, 'column', props: {'gap': 12, 'padding': 16}, children: [11, 12]),
    create(1, 'box', props: {'background': 'background'}, children: [10]),

    // Modal content
    create(
      21,
      'text',
      props: {'text': 'Channel Settings', 'size': 16, 'weight': 'bold'},
    ),
    create(
      22,
      'text',
      props: {
        'text':
            'Configure plugin channel permissions and notification settings.',
        'color': 'text_secondary',
      },
    ),
    create(23, 'checkbox', props: {'text': 'Enable audit log', 'value': true}),
    create(24, 'button', props: {'text': 'Done', 'variant': 'primary'}),
    create(
      20,
      'column',
      props: {'gap': 16, 'padding': 20},
      children: [21, 22, 23, 24],
    ),
    create(2, 'box', key: 'modal_node', children: [20]),

    roots(1),
  ];
}

void main() {
  for (final (name, theme) in themes) {
    for (final width in [800.0, 360.0]) {
      testWidgets('overlays_${name}_${width.toInt()}', (tester) async {
        final host = _GoldenHost();
        final controller = BlocksController();
        addTearDown(controller.dispose);
        controller.applyBatch(
          batch(1, [
            ..._overlaysPage(),
            {
              'op': 'roots',
              'main': 1,
              'modals': [
                {
                  'id': 2,
                  'name': 'settings_modal',
                  'size': 'medium',
                  'sheet': true,
                  'dismissible': true,
                },
              ],
            },
          ]),
        );

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
          // Advance past the 180 ms opening animation so modal is fully visible
          await tester.pump(const Duration(milliseconds: 200));
          await expectLater(
            find.byType(BlocksView),
            matchesGoldenFile('goldens/overlays_${name}_${width.toInt()}.png'),
          );
        });
      });
    }
  }
}
