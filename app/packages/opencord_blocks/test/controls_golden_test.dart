import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

class _GoldenHost extends FakeHost {
  @override
  IconData? icon(String name) => switch (name) {
    'star' => const IconData(0xf09a, fontFamily: 'MaterialSymbolsRounded'),
    'add' => const IconData(0xe145, fontFamily: 'MaterialSymbolsRounded'),
    'chat' => const IconData(0xe0c9, fontFamily: 'MaterialSymbolsRounded'),
    'folder' => const IconData(0xe2c7, fontFamily: 'MaterialSymbolsRounded'),
    _ => null,
  };
}

List<Map<String, Object?>> _controlsPage() {
  return [
    // 1. Buttons row
    create(
      11,
      'button',
      props: {'text': 'Primary', 'variant': 'primary', 'icon': 'star'},
    ),
    create(12, 'button', props: {'text': 'Secondary', 'variant': 'secondary'}),
    create(13, 'button', props: {'text': 'Danger', 'variant': 'danger'}),
    create(14, 'button', props: {'text': 'Plain', 'variant': 'plain'}),
    create(15, 'icon_button', props: {'icon': 'add'}),
    create(
      10,
      'row',
      props: {'gap': 8, 'cross': 'center', 'wrap': true},
      children: [11, 12, 13, 14, 15],
    ),

    // 2. Toggles row
    create(21, 'checkbox', props: {'value': true, 'text': 'Subscribed'}),
    create(22, 'checkbox', props: {'value': false, 'text': 'Mute'}),
    create(23, 'switch', props: {'value': true, 'text': 'Dark Mode'}),
    create(24, 'radio', props: {'value': true, 'text': 'Option A'}),
    create(25, 'radio', props: {'value': false, 'text': 'Option B'}),
    create(
      20,
      'row',
      props: {'gap': 12, 'cross': 'center', 'wrap': true},
      children: [21, 22, 23, 24, 25],
    ),

    // 3. Slider
    create(
      31,
      'slider',
      props: {'value': 0.65, 'min': 0.0, 'max': 1.0, 'width': 'fill'},
    ),
    create(30, 'box', props: {'width': 'fill'}, children: [31]),

    // 4. Select dropdown & Tabs
    create(
      41,
      'select',
      props: {
        'value': 'us-east',
        'options': [
          {'value': 'us-east', 'label': 'US East (N. Virginia)'},
          {'value': 'eu-west', 'label': 'EU West (Frankfurt)'},
        ],
        'width': 220,
      },
    ),
    create(
      42,
      'tabs',
      props: {
        'value': 'chat',
        'options': [
          {'value': 'chat', 'label': 'Chat', 'icon': 'chat'},
          {'value': 'files', 'label': 'Files', 'icon': 'folder'},
        ],
      },
    ),
    create(
      43,
      'segmented',
      props: {
        'value': 'grid',
        'options': [
          {'value': 'grid', 'label': 'Grid'},
          {'value': 'list', 'label': 'List'},
        ],
      },
    ),
    create(
      40,
      'row',
      props: {'gap': 12, 'cross': 'center', 'wrap': true},
      children: [41, 42, 43],
    ),

    // 5. Text inputs
    create(
      51,
      'text_input',
      props: {
        'value': 'Initial text value',
        'placeholder': 'Enter text...',
        'width': 'fill',
      },
    ),
    create(
      52,
      'text_input',
      props: {'placeholder': 'Multiline notes...', 'lines': 2, 'width': 'fill'},
    ),
    create(
      50,
      'column',
      props: {'gap': 8, 'width': 'fill'},
      children: [51, 52],
    ),

    // 6. Pickers row
    create(61, 'image_picker', props: {'text': 'Upload Cover'}),
    create(62, 'file_picker', props: {'text': 'Attach Log'}),
    create(
      60,
      'row',
      props: {'gap': 10, 'cross': 'center'},
      children: [61, 62],
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
      children: [10, 20, 30, 40, 50, 60],
    ),
    roots(1),
  ];
}

void main() {
  for (final (name, theme) in themes) {
    for (final width in [800.0, 360.0]) {
      testWidgets('controls_${name}_${width.toInt()}', (tester) async {
        final host = _GoldenHost();
        final controller = BlocksController();
        addTearDown(controller.dispose);
        controller.applyBatch(batch(1, _controlsPage()));

        await withRealShadows(() async {
          await pumpView(
            tester,
            controller,
            host: host,
            theme: theme,
            width: width,
            height: 800,
            window: Size(width, 800),
          );
          await tester.pump();
          await expectLater(
            find.byType(BlocksView),
            matchesGoldenFile('goldens/controls_${name}_${width.toInt()}.png'),
          );
        });
      });
    }
  }
}
