import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

/// The app's colours (`OcColors`, with `background` = `chat` and `danger` =
/// `recording`, as Task 24 maps them), copied so the package's tests stand
/// alone.
final darkTheme = BlocksTheme(
  brightness: Brightness.dark,
  colors: const {
    'text': Color(0xFFEDEDEF),
    'text_secondary': Color(0xFFA1A1A8),
    'text_muted': Color(0xFF6E6E76),
    'background': Color(0xFF0E0E10),
    'surface': Color(0xFF18181B),
    'elevated': Color(0xFF1F1F23),
    'border': Color(0xFF222226),
    'accent': Color(0xFFEDEDEF),
    'on_accent': Color(0xFF0E0E10),
    'selected': Color(0xFF27272C),
    'hover': Color(0xFF1C1C20),
    'danger': Color(0xFFF2555A),
  },
  text: const TextStyle(
    fontFamily: 'Inter',
    fontSize: 14,
    height: 20 / 14,
    color: Color(0xFFEDEDEF),
  ),
);

final lightTheme = BlocksTheme(
  brightness: Brightness.light,
  colors: const {
    'text': Color(0xFF111114),
    'text_secondary': Color(0xFF55555C),
    'text_muted': Color(0xFF8A8A92),
    'background': Color(0xFFF3F3F5),
    'surface': Color(0xFFFFFFFF),
    'elevated': Color(0xFFFFFFFF),
    'border': Color(0xFFE3E3E7),
    'accent': Color(0xFF111114),
    'on_accent': Color(0xFFFFFFFF),
    'selected': Color(0xFFE7E7EA),
    'hover': Color(0xFFF2F2F4),
    'danger': Color(0xFFDC3E42),
  },
  text: const TextStyle(
    fontFamily: 'Inter',
    fontSize: 14,
    height: 20 / 14,
    color: Color(0xFF111114),
  ),
);

final themes = [('dark', darkTheme), ('light', lightTheme)];

/// A host that records what the view asked of it.
class FakeHost implements BlocksHost {
  final calls = <String>[];
  final List<VoidCallback> soundEnds = [];
  bool linkAnswer = true;

  @override
  IconData? icon(String name) => null;

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async => null;

  @override
  Widget user(String id, double size, bool showName) =>
      SizedBox.square(dimension: size);

  @override
  Widget richText(String markdown, TextStyle style) =>
      Text(markdown, style: style);

  @override
  Future<String?> pickFile({
    required bool image,
    List<String> accept = const [],
  }) async => null;

  @override
  Future<bool> confirmLink(String url) async {
    calls.add('confirmLink $url');
    return linkAnswer;
  }

  @override
  void copy(String text) => calls.add('copy $text');

  @override
  void openChannel(String id) => calls.add('openChannel $id');

  @override
  Future<void> playSound(
    String asset,
    double volume,
    VoidCallback onEnd,
  ) async {
    calls.add('playSound $asset $volume');
    soundEnds.add(onEnd);
  }

  @override
  Future<ByteData?> font(String file) async => null;

  @override
  Future<void> upload(int nodeId, String path) async {
    calls.add('upload $nodeId $path');
  }
}

/// A batch as the core sends it.
String batch(int seq, List<Map<String, Object?>> ops) =>
    jsonEncode({'view': '1', 'seq': seq, 'ops': ops});

Map<String, Object?> create(
  int id,
  String block, {
  String? key,
  Map<String, Object?> props = const {},
  List<String> events = const [],
  List<int> children = const [],
}) => {
  'op': 'create',
  'id': id,
  'block': block,
  'key': ?key,
  'props': props,
  'events': events,
  'children': children,
};

Map<String, Object?> roots(int main) => {
  'op': 'roots',
  'main': main,
  'modals': const <Object?>[],
};

/// Everything the controller sends, decoded, as it arrives.
class Inputs {
  Inputs(BlocksController controller) {
    _subscription = controller.inputs.listen(
      (json) => all.add(jsonDecode(json) as Map<String, Object?>),
    );
  }

  final all = <Map<String, Object?>>[];
  late final StreamSubscription<String> _subscription;

  List<Map<String, Object?>> of(String kind) => [
    for (final input in all)
      if (input['kind'] == kind) input,
  ];

  Future<void> cancel() => _subscription.cancel();
}

/// Draws [controller] in a [width] × [height] plugin area at the top left of
/// an 800 × 600 window.
Future<void> pumpView(
  WidgetTester tester,
  BlocksController controller, {
  BlocksHost? host,
  BlocksTheme? theme,
  double width = 400,
  double height = 300,
  Size window = const Size(800, 600),
}) async {
  tester.view.physicalSize = window;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final shown = theme ?? darkTheme;
  await tester.pumpWidget(
    MaterialApp(
      debugShowCheckedModeBanner: false,
      home: ColoredBox(
        color: shown.colors['background']!,
        child: Align(
          alignment: Alignment.topLeft,
          child: SizedBox(
            width: width,
            height: height,
            child: BlocksView(
              controller: controller,
              host: host ?? FakeHost(),
              theme: shown,
            ),
          ),
        ),
      ),
    ),
  );
}

/// Runs [body] with shadows blurred as the app blurs them; Flutter's tests
/// turn the blur off, which draws a hard band under every shadow. The flag
/// is put back in `finally`, not in a tear-down: Flutter checks it is back
/// before tear-downs run, and fails the test otherwise.
Future<void> withRealShadows(Future<void> Function() body) async {
  debugDisableShadows = false;
  try {
    await body();
  } finally {
    debugDisableShadows = true;
  }
}

/// The rectangle a block's widget takes, found by its node id.
Rect blockRect(WidgetTester tester, int id) =>
    tester.getRect(find.byKey(ValueKey<int>(id)));
