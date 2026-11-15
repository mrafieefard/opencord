import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

/// A page built from the blocks this package draws so far: rows and
/// columns with gaps, alignment and wrapping; boxes with every container
/// prop; sizes, flex, opacity; and a block it doesn't know.
List<Map<String, Object?>> _page() {
  Map<String, Object?> chip(int id) => create(
    id,
    'box',
    props: {
      'height': 32,
      'width': 88 + (id % 3) * 12,
      'radius': 16,
      'background': 'selected',
      'border': {'width': 1, 'color': 'border'},
    },
  );
  return [
    // A header row: a card, a stretching accent bar, a round gradient.
    create(
      11,
      'box',
      props: {
        'width': 120,
        'height': 56,
        'radius': 8,
        'background': 'surface',
        'border': {'width': 1, 'color': 'border'},
        'shadow': 'small',
      },
    ),
    create(
      12,
      'box',
      props: {
        'width': 'fill',
        'height': 56,
        'radius': 8,
        'background': 'accent',
      },
    ),
    create(
      13,
      'box',
      props: {
        'width': 56,
        'height': 56,
        'radius': 28,
        'background': {'from': 'accent', 'to': '#4C8DFF', 'angle': 135},
      },
    ),
    create(10, 'row', props: {'gap': 8}, children: [11, 12, 13]),
    // Chips that wrap at narrow widths.
    for (var id = 21; id <= 28; id++) chip(id),
    create(
      20,
      'row',
      props: {'gap': 8, 'wrap': true},
      children: [for (var id = 21; id <= 28; id++) id],
    ),
    // A card holding a column of bars and an unknown block.
    create(
      41,
      'box',
      props: {
        'width': 160,
        'height': 8,
        'radius': 4,
        'background': 'text_muted',
      },
    ),
    create(
      42,
      'box',
      props: {
        'width': 'fill',
        'height': 8,
        'radius': 4,
        'background': 'text_secondary',
      },
    ),
    create(43, 'sparkle'),
    create(40, 'column', props: {'gap': 10}, children: [41, 42, 43]),
    create(
      30,
      'box',
      props: {
        'width': 'fill',
        'padding': 16,
        'radius': 12,
        'background': 'surface',
        'border': {'width': 1, 'color': 'border'},
        'shadow': 'medium',
      },
      children: [40],
    ),
    // Flex shares and fading dots, spread apart.
    create(
      51,
      'box',
      props: {'flex': 1, 'height': 24, 'radius': 6, 'background': 'danger'},
    ),
    create(
      52,
      'box',
      props: {
        'flex': 2,
        'height': 24,
        'radius': 6,
        'background': 'accent',
        'opacity': 0.6,
      },
    ),
    create(50, 'row', props: {'gap': 8}, children: [51, 52]),
    for (final (id, opacity) in [(61, 1.0), (62, 0.6), (63, 0.3)])
      create(
        id,
        'box',
        props: {
          'width': 40,
          'height': 40,
          'radius': 20,
          'background': 'accent',
          'opacity': opacity,
        },
      ),
    create(
      60,
      'row',
      props: {
        'align': 'space_between',
        // The core sends insets as a number or as all four sides.
        'padding': {'top': 0, 'right': 24, 'bottom': 0, 'left': 24},
      },
      children: [61, 62, 63],
    ),
    create(
      1,
      'column',
      props: {
        'gap': 16,
        'padding': 16,
        'width': 'fill',
        'height': 'fill',
        'background': 'background',
      },
      children: [10, 20, 30, 50, 60],
    ),
    roots(1),
  ];
}

void main() {
  for (final (name, theme) in themes) {
    for (final width in [800.0, 360.0]) {
      // Fails if anything in the sample page draws differently: a colour,
      // a radius, a shadow, a gap, a wrap, a flex share.
      testWidgets('the_sample_page_${name}_${width.toInt()}', (tester) async {
        final controller = BlocksController();
        addTearDown(controller.dispose);
        controller.applyBatch(batch(1, _page()));
        await withRealShadows(() async {
          await pumpView(
            tester,
            controller,
            theme: theme,
            width: width,
            height: 480,
            window: Size(width, 480),
          );
          await expectLater(
            find.byType(BlocksView),
            matchesGoldenFile('goldens/sample_${name}_${width.toInt()}.png'),
          );
        });
      });
    }
  }
}
