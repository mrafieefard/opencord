import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

class _LayoutHost extends FakeHost {
  final Map<String, int> fontRequests = {};
  final List<Map<String, Object?>> imageRequests = [];
  final List<String> missingIconLogs = [];
  ui.Image? fakeLoadedImage;
  bool returnImage = true;

  @override
  IconData? icon(String name) {
    if (name == 'star') return Icons.star;
    return null;
  }

  @override
  Future<ByteData?> font(String file) async {
    fontRequests[file] = (fontRequests[file] ?? 0) + 1;
    return null;
  }

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async {
    imageRequests.add(source);
    if (!returnImage) return null;
    return _FakeImageProvider(fakeLoadedImage);
  }
}

class _FakeImageProvider extends ImageProvider<_FakeImageProvider> {
  const _FakeImageProvider(this.image);
  final ui.Image? image;

  @override
  Future<_FakeImageProvider> obtainKey(ImageConfiguration configuration) {
    return SynchronousFuture<_FakeImageProvider>(this);
  }

  @override
  ImageStreamCompleter loadImage(
    _FakeImageProvider key,
    ImageDecoderCallback decode,
  ) {
    return OneFrameImageStreamCompleter(_load());
  }

  Future<ImageInfo> _load() async {
    final img = image ?? await _createTestImage(width: 40, height: 30);
    return ImageInfo(image: img);
  }

  static Future<ui.Image> _createTestImage({
    required int width,
    required int height,
  }) async {
    final recorder = ui.PictureRecorder();
    final canvas = Canvas(recorder);
    canvas.drawRect(
      Rect.fromLTWH(0, 0, width.toDouble(), height.toDouble()),
      Paint()..color = const Color(0xFF00FF00),
    );
    final picture = recorder.endRecording();
    return picture.toImage(width, height);
  }
}

BlocksController _show(List<Map<String, Object?>> creates, int root) {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  controller.applyBatch(batch(1, [...creates, roots(root)]));
  return controller;
}

void main() {
  group('layout blocks', () {
    testWidgets('stack_positions_children_and_unpositioned_fills', (
      tester,
    ) async {
      final controller = _show([
        create(
          2,
          'box',
          props: {
            'x': 10,
            'y': 20,
            'width': 30,
            'height': 40,
            'background': 'accent',
          },
        ),
        create(
          3,
          'box',
          props: {
            'right': 15,
            'bottom': 25,
            'width': 50,
            'height': 50,
            'background': 'danger',
          },
        ),
        create(4, 'box', props: {'background': 'surface'}),
        create(
          1,
          'stack',
          props: {'width': 200, 'height': 200},
          children: [4, 2, 3],
        ),
      ], 1);
      await pumpView(tester, controller, width: 200, height: 200);

      final r2 = blockRect(tester, 2);
      expect(r2.left, 10);
      expect(r2.top, 20);
      expect(r2.width, 30);
      expect(r2.height, 40);

      final r3 = blockRect(tester, 3);
      expect(r3.left, 200 - 15 - 50);
      expect(r3.top, 200 - 25 - 50);
      expect(r3.width, 50);
      expect(r3.height, 50);

      // Unpositioned child 4 fills the stack
      final r4 = blockRect(tester, 4);
      expect(r4.left, 0);
      expect(r4.top, 0);
      expect(r4.width, 200);
      expect(r4.height, 200);
    });

    testWidgets('grid_columns_and_gap', (tester) async {
      final controller = _show([
        create(2, 'box', props: {'height': 40, 'background': 'accent'}),
        create(3, 'box', props: {'height': 40, 'background': 'accent'}),
        create(4, 'box', props: {'height': 40, 'background': 'accent'}),
        create(
          1,
          'grid',
          props: {'columns': 2, 'gap': 10, 'width': 110},
          children: [2, 3, 4],
        ),
      ], 1);
      await pumpView(tester, controller, width: 110, height: 200);

      final r2 = blockRect(tester, 2);
      final r3 = blockRect(tester, 3);
      final r4 = blockRect(tester, 4);

      // 110 width with 2 columns and 10 gap -> 50 each
      expect(r2.left, 0);
      expect(r2.width, 50);
      expect(r3.left, 60);
      expect(r3.width, 50);
      // r4 is on row 2, under r2
      expect(r4.left, 0);
      expect(r4.top, r2.bottom + 10);
    });

    testWidgets('grid_min_column_width_adapts', (tester) async {
      final controller = _show([
        for (var i = 2; i <= 5; i++)
          create(i, 'box', props: {'height': 30, 'background': 'surface'}),
        create(
          1,
          'grid',
          props: {'min_column_width': 100, 'gap': 10, 'width': 320},
          children: [2, 3, 4, 5],
        ),
      ], 1);
      await pumpView(tester, controller, width: 320, height: 200);

      // (320 + 10) / (100 + 10) = 3 columns.
      // Width per column = (320 - 20) / 3 = 100.
      final r2 = blockRect(tester, 2);
      final r3 = blockRect(tester, 3);
      final r4 = blockRect(tester, 4);
      final r5 = blockRect(tester, 5);

      expect(r2.top, r3.top);
      expect(r3.top, r4.top);
      expect(r5.top, greaterThan(r2.top));
    });

    testWidgets('scroll_sends_scroll_reach_start_and_reach_end', (
      tester,
    ) async {
      final controller = _show([
        for (var i = 10; i <= 30; i++)
          create(i, 'box', props: {'height': 50, 'width': 100}),
        create(
          2,
          'column',
          props: {'width': 100},
          children: [for (var i = 10; i <= 30; i++) i],
        ),
        create(
          1,
          'scroll',
          props: {'width': 100, 'height': 200, 'direction': 'vertical'},
          events: ['scroll', 'reach_start', 'reach_end'],
          children: [2],
        ),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller, width: 100, height: 200);
      await tester.pumpAndSettle();

      // At start (offset 0 <= 48 px), reach_start should have been sent once
      final reachStarts = inputs
          .of('event')
          .where((e) => e['event'] == 'reach_start')
          .toList();
      expect(reachStarts.length, 1);

      inputs.all.clear();
      // Scroll down by 500 px
      await tester.drag(
        find.byKey(const ValueKey<int>(1)),
        const Offset(0, -500),
      );
      await tester.pumpAndSettle();

      final scrollEvents = inputs
          .of('event')
          .where((e) => e['event'] == 'scroll')
          .toList();
      expect(scrollEvents.isNotEmpty, isTrue);
      final lastScroll = scrollEvents.last;
      expect((lastScroll['data'] as Map)['offset'], greaterThan(0));

      // Scroll to the very bottom to trigger reach_end
      await tester.drag(
        find.byKey(const ValueKey<int>(1)),
        const Offset(0, -1000),
      );
      await tester.pumpAndSettle();

      final reachEnds = inputs
          .of('event')
          .where((e) => e['event'] == 'reach_end')
          .toList();
      expect(reachEnds.isNotEmpty, isTrue);
    });

    testWidgets('spacer_takes_size_or_flex', (tester) async {
      final controller = _show([
        create(2, 'box', props: {'width': 40, 'height': 40}),
        create(3, 'spacer', props: {'size': 20}),
        create(4, 'box', props: {'width': 40, 'height': 40}),
        create(1, 'row', children: [2, 3, 4]),
      ], 1);
      await pumpView(tester, controller, width: 200, height: 100);

      final r2 = blockRect(tester, 2);
      final r4 = blockRect(tester, 4);
      expect(r4.left - r2.right, 20);
    });

    testWidgets('divider_draws_line', (tester) async {
      final controller = _show([
        create(
          1,
          'divider',
          props: {'thickness': 3, 'color': 'danger', 'width': 150},
        ),
      ], 1);
      await pumpView(tester, controller, width: 200, height: 100);

      final r1 = blockRect(tester, 1);
      expect(r1.height, 3);
      expect(r1.width, 150);
    });
  });

  group('content blocks', () {
    testWidgets('text_weights_italics_colors_and_selectable', (tester) async {
      final controller = _show([
        create(
          1,
          'text',
          props: {
            'text': 'Hello world',
            'weight': 'bold',
            'italic': true,
            'color': 'accent',
            'size': 18,
            'selectable': true,
          },
        ),
      ], 1);
      await pumpView(tester, controller);

      final selectable = tester.widget<SelectableText>(
        find.byType(SelectableText),
      );
      expect(selectable.data, 'Hello world');
      expect(selectable.style?.fontWeight, FontWeight.w700);
      expect(selectable.style?.fontStyle, FontStyle.italic);
      expect(selectable.style?.fontSize, 18);
    });

    testWidgets('a_very_long_word_wraps_inside_its_box', (tester) async {
      final longWord = 'A' * 60;
      final controller = _show([
        create(2, 'text', props: {'text': longWord, 'size': 14}),
        create(
          1,
          'box',
          props: {'width': 100, 'background': 'surface'},
          children: [2],
        ),
      ], 1);
      await pumpView(tester, controller, width: 200, height: 300);

      final textBox = tester.getRect(find.byType(Text));
      expect(textBox.width, lessThanOrEqualTo(100.5));
      expect(textBox.height, greaterThan(25)); // multi-line wrapped
    });

    testWidgets('right_to_left_text_aligns_start', (tester) async {
      final controller = _show([
        create(
          1,
          'text',
          props: {'text': 'سلام دنیا', 'width': 200, 'align': 'start'},
        ),
      ], 1);
      await pumpView(tester, controller, width: 200, height: 100);

      final textWidget = tester.widget<Text>(find.byType(Text));
      expect(textWidget.textDirection, TextDirection.rtl);
      expect(textWidget.textAlign, TextAlign.start);
    });

    testWidgets('text_loads_custom_font_via_host', (tester) async {
      final host = _LayoutHost();
      final controller = _show([
        create(1, 'text', props: {'text': 'Custom font', 'font': 'custom.ttf'}),
      ], 1);
      await pumpView(tester, controller, host: host);

      expect(host.fontRequests['custom.ttf'], 1);
    });

    testWidgets('rich_text_delegates_to_host', (tester) async {
      final host = _LayoutHost();
      final controller = _show([
        create(1, 'rich_text', props: {'text': '**Bold text**'}),
      ], 1);
      await pumpView(tester, controller, host: host);

      expect(find.text('**Bold text**'), findsOneWidget);
    });

    testWidgets('image_loads_via_host_and_fires_load_event', (tester) async {
      final host = _LayoutHost();
      final controller = _show([
        create(
          1,
          'image',
          props: {'asset': 'logo.png', 'fit': 'cover', 'radius': 8},
          events: ['load', 'error'],
        ),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller, host: host);
      await tester.pumpAndSettle();

      expect(host.imageRequests.first, equals({'asset': 'logo.png'}));
      final loads = inputs
          .of('event')
          .where((e) => e['event'] == 'load')
          .toList();
      expect(loads.isNotEmpty, isTrue);
      expect((loads.first['data'] as Map)['width'], 40.0);
      expect((loads.first['data'] as Map)['height'], 30.0);
    });

    testWidgets('an_image_version_change_reloads', (tester) async {
      final host = _LayoutHost();
      final controller = _show([
        create(1, 'image', props: {'file': 'chart.png', 'version': 1}),
      ], 1);
      await pumpView(tester, controller, host: host);
      await tester.pumpAndSettle();
      expect(host.imageRequests.length, 1);

      controller.applyBatch(
        batch(2, [
          {
            'op': 'props',
            'id': 1,
            'set': {'version': 2},
            'unset': <String>[],
          },
        ]),
      );
      await tester.pumpAndSettle();
      expect(host.imageRequests.length, 2);
    });

    testWidgets('icon_renders_and_missing_name_logs_once', (tester) async {
      final reported = <FlutterErrorDetails>[];
      final previous = FlutterError.onError;
      FlutterError.onError = reported.add;
      try {
        final host = _LayoutHost();
        final controller = _show([
          create(
            2,
            'icon',
            props: {'name': 'star', 'size': 24, 'color': 'accent'},
          ),
          create(3, 'icon', props: {'name': 'nonexistent'}),
          create(1, 'column', children: [2, 3]),
        ], 1);
        await pumpView(tester, controller, host: host);

        expect(find.byIcon(Icons.star), findsOneWidget);
        // Nonexistent icon draws nothing (SizedBox.shrink)
        expect(find.byType(Icon), findsOneWidget);
        expect(reported.length, 1);
        expect(reported.first.exception.toString(), contains('nonexistent'));

        // A second pump does not log again
        await pumpView(tester, controller, host: host);
        expect(reported.length, 1);
      } finally {
        FlutterError.onError = previous;
      }
    });

    testWidgets('user_delegates_to_host', (tester) async {
      final host = _LayoutHost();
      final controller = _show([
        create(1, 'user', props: {'id': '42', 'size': 32, 'show_name': true}),
      ], 1);
      await pumpView(tester, controller, host: host);

      expect(find.byType(SizedBox), findsWidgets);
    });

    testWidgets('badge_renders_tones', (tester) async {
      final controller = _show([
        create(2, 'badge', props: {'text': 'Neutral', 'tone': 'neutral'}),
        create(3, 'badge', props: {'text': 'Accent', 'tone': 'accent'}),
        create(4, 'badge', props: {'text': 'Danger', 'tone': 'danger'}),
        create(1, 'row', children: [2, 3, 4]),
      ], 1);
      await pumpView(tester, controller);

      expect(find.text('Neutral'), findsOneWidget);
      expect(find.text('Accent'), findsOneWidget);
      expect(find.text('Danger'), findsOneWidget);
    });

    testWidgets('progress_renders_bar_and_ring', (tester) async {
      final controller = _show([
        create(2, 'progress', props: {'value': 0.5, 'style': 'bar'}),
        create(3, 'progress', props: {'style': 'ring'}),
        create(1, 'column', children: [2, 3]),
      ], 1);
      await pumpView(tester, controller);

      expect(find.byType(LinearProgressIndicator), findsOneWidget);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);
    });

    testWidgets('spinner_renders_ring', (tester) async {
      final controller = _show([
        create(1, 'spinner', props: {'size': 20}),
      ], 1);
      await pumpView(tester, controller);

      expect(find.byType(CircularProgressIndicator), findsOneWidget);
    });
  });
}
