import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

class _GoldenHost extends FakeHost {
  _GoldenHost(this.testImage);
  final ui.Image testImage;

  @override
  IconData? icon(String name) => switch (name) {
    'star' => Icons.star,
    'tag' => Icons.tag,
    'person' => Icons.person,
    _ => null,
  };

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async {
    return _GoldenImageProvider(testImage);
  }

  @override
  Widget user(String id, double size, bool showName) {
    final avatar = Container(
      width: size,
      height: size,
      decoration: const BoxDecoration(
        color: Color(0xFF4C8DFF),
        shape: BoxShape.circle,
      ),
      alignment: Alignment.center,
      child: Text(
        id.isNotEmpty ? id.substring(0, 1).toUpperCase() : '?',
        style: TextStyle(
          color: Colors.white,
          fontSize: size * 0.45,
          fontWeight: FontWeight.bold,
        ),
      ),
    );
    if (!showName) return avatar;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        avatar,
        const SizedBox(width: 8),
        Text('User $id', style: const TextStyle(fontWeight: FontWeight.w500)),
      ],
    );
  }

  @override
  Widget richText(String markdown, TextStyle style) {
    return Text.rich(
      TextSpan(
        children: [
          TextSpan(text: 'Markdown: ', style: style),
          TextSpan(
            text: 'bold & code',
            style: style.copyWith(fontWeight: FontWeight.bold),
          ),
        ],
      ),
    );
  }
}

class _GoldenImageProvider extends ImageProvider<_GoldenImageProvider> {
  const _GoldenImageProvider(this.image);
  final ui.Image image;

  @override
  Future<_GoldenImageProvider> obtainKey(ImageConfiguration configuration) {
    return SynchronousFuture<_GoldenImageProvider>(this);
  }

  @override
  ImageStreamCompleter loadImage(
    _GoldenImageProvider key,
    ImageDecoderCallback decode,
  ) {
    return OneFrameImageStreamCompleter(Future.value(ImageInfo(image: image)));
  }
}

Future<ui.Image> _createTestImage() async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder);
  canvas.drawRect(
    const Rect.fromLTWH(0, 0, 80, 80),
    Paint()..color = const Color(0xFF388E3C),
  );
  canvas.drawCircle(
    const Offset(40, 40),
    25,
    Paint()..color = const Color(0xFFFFEB3B),
  );
  final picture = recorder.endRecording();
  return picture.toImage(80, 80);
}

List<Map<String, Object?>> _layoutContentPage() {
  return [
    // 1. Text header & badges
    create(
      11,
      'text',
      props: {
        'text': 'Block Showcase',
        'size': 20,
        'weight': 'bold',
        'color': 'accent',
      },
    ),
    create(12, 'badge', props: {'text': 'Active', 'tone': 'accent'}),
    create(13, 'badge', props: {'text': 'Draft', 'tone': 'neutral'}),
    create(14, 'badge', props: {'text': 'Alert', 'tone': 'danger'}),
    create(
      10,
      'row',
      props: {'gap': 8, 'cross': 'center'},
      children: [11, 12, 13, 14],
    ),

    // 2. Rich text & Icons
    create(
      21,
      'rich_text',
      props: {'text': '**Important:** rich text preview'},
    ),
    create(22, 'icon', props: {'name': 'star', 'size': 20, 'color': 'accent'}),
    create(
      23,
      'icon',
      props: {'name': 'tag', 'size': 20, 'color': 'text_secondary'},
    ),
    create(24, 'spinner', props: {'size': 20}),
    create(
      20,
      'row',
      props: {'gap': 12, 'cross': 'center'},
      children: [21, 22, 23, 24],
    ),

    // Divider
    create(25, 'divider', props: {'thickness': 1, 'color': 'border'}),

    // 3. Grid of cards with user, image, progress
    create(31, 'user', props: {'id': 'alice', 'size': 32, 'show_name': true}),
    create(
      32,
      'image',
      props: {
        'asset': 'test.png',
        'width': 60,
        'height': 60,
        'radius': 8,
        'fit': 'cover',
      },
    ),
    create(
      33,
      'box',
      props: {
        'background': 'surface',
        'padding': 8,
        'radius': 8,
        'border': {'width': 1, 'color': 'border'},
      },
      children: [31],
    ),
    create(
      34,
      'box',
      props: {
        'background': 'surface',
        'padding': 8,
        'radius': 8,
        'border': {'width': 1, 'color': 'border'},
      },
      children: [32],
    ),
    create(30, 'grid', props: {'columns': 2, 'gap': 12}, children: [33, 34]),

    // 4. Progress indicators
    create(
      41,
      'progress',
      props: {'value': 0.7, 'style': 'bar', 'width': 'fill'},
    ),
    create(42, 'progress', props: {'value': 0.4, 'style': 'ring'}),
    create(43, 'progress', props: {'style': 'ring'}),
    create(
      40,
      'row',
      props: {'gap': 12, 'cross': 'center'},
      children: [41, 42, 43],
    ),

    // 5. Stack with positioned and unpositioned children
    create(
      51,
      'box',
      props: {
        'background': 'surface',
        'radius': 8,
        'border': {'width': 1, 'color': 'border'},
      },
    ),
    create(
      52,
      'text',
      props: {
        'text': 'Top-Left',
        'x': 8,
        'y': 8,
        'size': 11,
        'color': 'text_muted',
      },
    ),
    create(
      53,
      'badge',
      props: {'text': 'Overlay', 'tone': 'accent', 'right': 8, 'bottom': 8},
    ),
    create(
      50,
      'stack',
      props: {'width': 'fill', 'height': 60},
      children: [51, 52, 53],
    ),

    // 6. Text styling & wrapping
    create(61, 'text', props: {'text': 'Regular 14px Inter text'}),
    create(
      62,
      'text',
      props: {
        'text': 'Medium 14px text',
        'weight': 'medium',
        'color': 'text_secondary',
      },
    ),
    create(
      63,
      'text',
      props: {
        'text': 'Semibold italic text',
        'weight': 'semibold',
        'italic': true,
      },
    ),
    create(
      64,
      'text',
      props: {'text': 'سلام دنیا - RTL alignment', 'align': 'start'},
    ),
    create(60, 'column', props: {'gap': 6}, children: [61, 62, 63, 64]),

    // Main container column
    create(
      1,
      'column',
      props: {
        'gap': 14,
        'padding': 16,
        'width': 'fill',
        'background': 'background',
      },
      children: [10, 20, 25, 30, 40, 50, 60],
    ),
    roots(1),
  ];
}

void main() {
  for (final (name, theme) in themes) {
    for (final width in [800.0, 360.0]) {
      testWidgets('layout_content_${name}_${width.toInt()}', (tester) async {
        final testImage = await _createTestImage();
        final host = _GoldenHost(testImage);
        final controller = BlocksController();
        addTearDown(controller.dispose);
        controller.applyBatch(batch(1, _layoutContentPage()));

        await withRealShadows(() async {
          await pumpView(
            tester,
            controller,
            host: host,
            theme: theme,
            width: width,
            height: 520,
            window: Size(width, 520),
          );
          await tester.pump();
          await expectLater(
            find.byType(BlocksView),
            matchesGoldenFile(
              'goldens/layout_content_${name}_${width.toInt()}.png',
            ),
          );
        });
      });
    }
  }
}
