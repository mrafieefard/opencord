// The prototype's scenes (`.superpowers/sdd/floating-ui/reference/`),
// reached on the mock world the way a person reaches them: taps and hovers
// only. test/showcase/floating_review_test.dart and floating_film_test.dart
// render them for the floating UI's checkpoints.
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' show ImageByteFormat, PictureRecorder;

import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/voice/voice_tile.dart';

import 'app.dart';
import 'finders.dart';

/// Runs [body] with shadows blurred as the app blurs them. Flutter's tests
/// turn the blur off, which leaves a hard band under every float.
Future<void> withRealShadows(Future<void> Function() body) async {
  debugDisableShadows = false;
  try {
    await body();
  } finally {
    debugDisableShadows = true;
  }
}

/// A window size the checks render at (the plan's breakpoints).
class Breakpoint {
  const Breakpoint(this.tag, this.size);

  final String tag;
  final Size size;
}

const wide1440 = Breakpoint('wide-1440', Size(1440, 900));
const wide1280 = Breakpoint('wide-1280', Size(1280, 800));
const wide1180 = Breakpoint('wide-1180', Size(1180, 800));
const medium1100 = Breakpoint('medium-1100', Size(1100, 800));
const medium980 = Breakpoint('medium-980', Size(980, 800));
const narrow940 = Breakpoint('narrow-940', Size(940, 560));
const compact759 = Breakpoint('compact-759', Size(759, 700));

const breakpoints = [
  wide1440,
  wide1280,
  wide1180,
  medium1100,
  medium980,
  narrow940,
  compact759,
];

/// A scene of the prototype's, with the widths it was taken at, dark at
/// every one of [widths] and light at [light].
class FloatingScene {
  const FloatingScene(
    this.name,
    this.play, {
    this.widths = breakpoints,
    this.light = const [wide1280, medium980],
  });

  final String name;
  final Future<void> Function(SceneDriver scene) play;
  final List<Breakpoint> widths;
  final List<Breakpoint> light;
}

/// Every scene in `reference/` but `me-menu`, the prototype's stand-in for
/// what the real cog does (User settings).
const floatingScenes = [
  FloatingScene('text', playText),
  FloatingScene('text-pane-closed', playTextPaneClosed),
  FloatingScene('voice', playVoice),
  FloatingScene('voice-focus', playVoiceFocus),
  FloatingScene('music', playMusic),
  FloatingScene('music-playing', playMusicPlaying),
  FloatingScene('dm', playDm),
  FloatingScene('friends', playFriends),
  FloatingScene('pending', playPending),
  FloatingScene('requests', playRequests),
  FloatingScene('server-menu', playServerMenu),
  FloatingScene('homelab', playHomelab),
  FloatingScene('rail-pop-dev', playRailPopDev, widths: [wide1280], light: []),
  FloatingScene(
    'rail-pop-rust',
    playRailPopRust,
    widths: [wide1280],
    light: [],
  ),
  FloatingScene(
    'rail-pop-homelab',
    playRailPopHomelab,
    widths: [wide1280],
    light: [],
  ),
  FloatingScene(
    'rail-pop-private',
    playRailPopPrivate,
    widths: [wide1280],
    light: [],
  ),
  FloatingScene(
    'drawer-right',
    playDrawerRight,
    widths: [medium1100],
    light: [],
  ),
  FloatingScene('drawer-left', playDrawerLeft, widths: [compact759], light: []),
];

/// What a scene needs that the app doesn't have yet. The scene is skipped
/// and says which.
class SceneUnavailable implements Exception {
  const SceneUnavailable(this.what);

  final String what;

  @override
  String toString() => 'nothing to reach for $what';
}

/// Taps, hovers and waits, as a person does.
class SceneDriver {
  SceneDriver(this.tester);

  final WidgetTester tester;
  TestGesture? _mouse;

  /// Lets [ms] of animation run, in frames.
  Future<void> settle([int ms = 700]) async {
    for (var waited = 0; waited < ms; waited += 50) {
      await tester.pump(const Duration(milliseconds: 50));
    }
  }

  /// Whether any of [finders] finds something the pointer can reach.
  bool shows(List<Finder> finders) =>
      finders.any((finder) => finder.hitTestable().evaluate().isNotEmpty);

  /// The first thing the pointer can reach through the first of [finders]
  /// that finds one. Each later finder is an older name for [what].
  Finder reach(String what, List<Finder> finders) {
    for (final finder in finders) {
      final hit = finder.hitTestable();
      if (hit.evaluate().isNotEmpty) return hit.first;
    }
    throw SceneUnavailable(what);
  }

  /// Taps [what], without waiting for what it starts.
  Future<void> tap(String what, List<Finder> finders) =>
      tester.tap(reach(what, finders));

  /// Moves the mouse over [what], without waiting.
  Future<void> hover(String what, List<Finder> finders) =>
      hoverAt(tester.getCenter(reach(what, finders)));

  Future<void> hoverAt(Offset position) async {
    var mouse = _mouse;
    if (mouse == null) {
      mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: position);
      addTearDown(mouse.removePointer);
      _mouse = mouse;
    }
    await mouse.moveTo(position);
  }

  /// Taps [text] in the left column, scrolling to it as needed. Where the
  /// window is too narrow for the column, opens its drawer first and closes
  /// it again if picking left it open.
  Future<void> pickInLeft(String text) async {
    final row = inLeft(text);
    var opened = false;
    if (!shows([find.byKey(ShellKeys.left)])) {
      await openLeftDrawer();
      opened = true;
    }
    final scrollBack = await revealInLeft(text);
    try {
      await tap(text, row);
      await settle();
    } finally {
      scrollBack();
    }
    await settle();
    if (opened && shows([find.byKey(ShellKeys.left)])) {
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await settle();
    }
  }

  /// Scrolls the left column's lists down until [text] shows in it, and
  /// returns what scrolls them back.
  ///
  /// Steps of 100, as a mouse wheel does: a drag would move the channel
  /// under the pointer instead.
  Future<VoidCallback> revealInLeft(String text) async {
    final row = inLeft(text);
    final lists = [
      for (final element
          in find
              .descendant(
                of: find.byKey(ShellKeys.left),
                matching: find.byType(Scrollable),
              )
              .evaluate())
        if ((element as StatefulElement).state case final ScrollableState list
            when list.position.axis == Axis.vertical)
          list,
    ];
    final before = {for (final list in lists) list: list.position.pixels};
    for (final list in lists) {
      final position = list.position;
      while (!shows(row) && position.pixels < position.maxScrollExtent) {
        position.jumpTo(
          math.min(position.pixels + 100, position.maxScrollExtent),
        );
        await tester.pump();
      }
      if (shows(row)) break;
    }
    return () {
      for (final MapEntry(key: list, value: pixels) in before.entries) {
        if (list.mounted) list.position.jumpTo(pixels);
      }
    };
  }

  Future<void> openLeftDrawer() async {
    await tap('the button that opens the channels', [
      find.byTooltip('Open channels'),
      find.byTooltip('Channels'),
    ]);
    await settle();
  }

  /// Opens the right pane where it is shut, then picks [tab] in it:
  /// `People` or `Music`.
  Future<void> paneOn(String tab) async {
    if (!shows(paneTab(tab))) await openPane();
    await tap('$tab in the right pane', paneTab(tab));
    await settle();
  }

  /// [text] in the left column.
  List<Finder> inLeft(String text) => [
    find.descendant(of: find.byKey(ShellKeys.left), matching: find.text(text)),
  ];

  /// `People` or `Music` in the right pane's switch, or today's buttons for
  /// them: in the pane's header, else in the page's.
  List<Finder> paneTab(String tab) => [
    find.descendant(of: find.byKey(ShellKeys.panel), matching: find.text(tab)),
    find.descendant(
      of: find.byKey(ShellKeys.panel),
      matching: find.byTooltip(tab == 'People' ? 'Members' : tab),
    ),
    find.descendant(
      of: find.byKey(ShellKeys.main),
      matching: find.byTooltip(tab == 'People' ? 'Members' : tab),
    ),
  ];

  /// The pane button, or the header's Members button before it.
  Future<void> openPane() async {
    await tapPaneButton();
    await settle();
  }

  Future<void> tapPaneButton() => tap('the button that opens the right pane', [
    find.byTooltip('Open the right pane'),
    find.byTooltip('Members'),
  ]);

  /// The right pane's close button, when the pane is open.
  List<Finder> get paneClose => [
    find.descendant(
      of: find.byKey(ShellKeys.panel),
      matching: find.byTooltip('Close'),
    ),
    find.descendant(
      of: find.byKey(ShellKeys.panel),
      matching: find.byTooltip('Close member list'),
    ),
  ];

  /// The server's head in the left column, or today's header's name.
  List<Finder> get serverHead => [
    find.byKey(ShellKeys.head),
    find.descendant(
      of: find.byKey(ShellKeys.left),
      matching: find.text('Opencord Dev'),
    ),
  ];

  /// The voice card's leave button.
  List<Finder> get voiceCardLeave => [
    find.descendant(
      of: find.byKey(ShellKeys.vcard),
      matching: find.byTooltip('Disconnect'),
    ),
    find.descendant(
      of: find.byKey(ShellKeys.left),
      matching: find.byTooltip('Disconnect'),
    ),
  ];

  /// The tile shown large in focus mode.
  List<Finder> get focusedTile => [
    find.byWidgetPredicate((w) => w is VoiceTileView && w.focused),
  ];

  /// Fails the scene when [what] isn't on screen.
  void need(String what, List<Finder> finders) => reach(what, finders);
}

/// Joined General, then #general: where every other scene starts, as the
/// prototype does.
Future<void> playText(SceneDriver scene) async {
  await scene.pickInLeft('General');
  await scene.settle(1500);
  await scene.pickInLeft('general');
}

Future<void> playTextPaneClosed(SceneDriver scene) async {
  await playText(scene);
  if (scene.shows(scene.paneClose)) {
    await scene.tap('the pane\'s close button', scene.paneClose);
    await scene.settle();
  }
}

/// The voice page, without the pointer over it.
Future<void> openVoicePage(SceneDriver scene) async {
  await playText(scene);
  await scene.pickInLeft('General');
  await scene.settle(800);
}

Future<void> playVoice(SceneDriver scene) async {
  await openVoicePage(scene);
  await scene.hoverAt(const Offset(600, 400));
  await scene.settle(200);
}

/// Watching Tomás's stream, which opens it large, without the pointer over
/// the page.
Future<void> watchStream(SceneDriver scene) async {
  await openVoicePage(scene);
  await scene.tap('Watch', [find.text('Watch')]);
  await scene.settle(1500);
}

Future<void> playVoiceFocus(SceneDriver scene) async {
  await watchStream(scene);
  await scene.hoverAt(const Offset(600, 300));
  await scene.settle(200);
}

Future<void> playMusic(SceneDriver scene) async {
  await openVoicePage(scene);
  await scene.paneOn('Music');
  await scene.settle(600);
}

Future<void> playMusicPlaying(SceneDriver scene) async {
  await playMusic(scene);
  final tester = scene.tester;
  final search = find.bySemanticsLabel('Search for music');
  if (search.evaluate().isNotEmpty) {
    await tester.enterText(search.first, 'lofi');
    await tester.testTextInput.receiveAction(TextInputAction.search);
    await scene.settle(600);
    final adds = find.byTooltip('Add to queue');
    final count = adds.evaluate().length;
    for (var i = 0; i < math.min(2, count); i++) {
      await tester.ensureVisible(adds.at(i));
      await tester.pump();
      await tester.tap(adds.at(i), warnIfMissed: false);
      await scene.settle(300);
    }
    final back = find.text('Back to queue');
    if (back.evaluate().isNotEmpty) {
      await tester.tap(back.first);
      await scene.settle(600);
    }
  }
}

Future<void> openPrivate(SceneDriver scene) async {
  await playText(scene);
  await scene.tap('the private messages on the rail', [railItem('Private')]);
  await scene.settle();
}

Future<void> playDm(SceneDriver scene) async {
  await openPrivate(scene);
  await scene.pickInLeft('Mira Okafor');
}

Future<void> playFriends(SceneDriver scene) async {
  await openPrivate(scene);
  await scene.pickInLeft('Friends');
}

Future<void> playPending(SceneDriver scene) async {
  await playFriends(scene);
  await scene.tap('the Pending tab', [
    find.descendant(
      of: find.byKey(ShellKeys.main),
      matching: find.text('Pending'),
    ),
  ]);
  await scene.settle();
}

Future<void> playRequests(SceneDriver scene) async {
  await openPrivate(scene);
  await scene.pickInLeft('Message requests');
}

Future<void> playServerMenu(SceneDriver scene) async {
  await playText(scene);
  if (!scene.shows(scene.serverHead)) await scene.openLeftDrawer();
  await scene.tap('the server\'s head', scene.serverHead);
  await scene.settle();
}

Future<void> playHomelab(SceneDriver scene) async {
  await playText(scene);
  await scene.tap('Homelab on the rail', [railItem('Homelab')]);
  await scene.settle();
}

Future<void> _railPop(SceneDriver scene, String server) async {
  await playText(scene);
  await scene.hover('$server on the rail', [railItem(server)]);
  await scene.settle(500);
  scene.need('the rail\'s popover', [find.byKey(ShellKeys.railPop)]);
}

Future<void> playRailPopDev(SceneDriver scene) =>
    _railPop(scene, 'Opencord Dev');

Future<void> playRailPopRust(SceneDriver scene) =>
    _railPop(scene, 'Rust Berlin');

Future<void> playRailPopHomelab(SceneDriver scene) =>
    _railPop(scene, 'Homelab');

Future<void> playRailPopPrivate(SceneDriver scene) =>
    _railPop(scene, 'Private');

Future<void> playDrawerRight(SceneDriver scene) async {
  await playText(scene);
  await scene.openPane();
}

Future<void> playDrawerLeft(SceneDriver scene) async {
  await playText(scene);
  await scene.openLeftDrawer();
}

/// Lets pictures decode, which happens outside the test's clock.
Future<void> decodeImages(WidgetTester tester) async {
  for (var i = 0; i < 5; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 100)),
    );
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// Writes the window at twice its size to [path] as a PNG, or only [clip]
/// of it, given in logical pixels.
Future<void> writeShot(WidgetTester tester, String path, {Rect? clip}) async {
  final boundary =
      MockApp.boundaryKey.currentContext!.findRenderObject()!
          as RenderRepaintBoundary;
  final bytes = await tester.runAsync(() async {
    var image = await boundary.toImage(pixelRatio: 2);
    if (clip != null) {
      final source = Rect.fromLTWH(
        clip.left * 2,
        clip.top * 2,
        clip.width * 2,
        clip.height * 2,
      );
      final recorder = PictureRecorder();
      Canvas(
        recorder,
      ).drawImageRect(image, source, Offset.zero & source.size, Paint());
      final picture = recorder.endRecording();
      final cropped = await picture.toImage(
        source.width.round(),
        source.height.round(),
      );
      picture.dispose();
      image.dispose();
      image = cropped;
    }
    final data = await image.toByteData(format: ImageByteFormat.png);
    image.dispose();
    return data;
  });
  File(path).writeAsBytesSync(bytes!.buffer.asUint8List());
}

/// Each [ShellKeys] frame on screen as `[left, top, width, height]`,
/// rounded to 0.1, under its name in §6.
Map<String, List<double>> measureFrames(WidgetTester tester) {
  List<double> rounded(Rect rect) => [
    for (final value in [rect.left, rect.top, rect.width, rect.height])
      (value * 10).round() / 10,
  ];
  return {
    for (final MapEntry(key: name, value: key) in ShellKeys.byName.entries)
      if (find.byKey(key).evaluate().isNotEmpty)
        name: rounded(tester.getRect(find.byKey(key).first)),
  };
}

/// Draws emoji with the colour font at [path], as the system would: tests
/// have no system fonts. Any colour emoji font works; it takes the first
/// family in `OcText`'s emoji fallbacks.
Future<void> loadEmojiFont(String path) async {
  if (path.isEmpty) return;
  final loader = FontLoader('Noto Color Emoji')
    ..addFont(File(path).readAsBytes().then(ByteData.sublistView));
  await loader.load();
}

/// A comma-separated `--dart-define` list, or null when it isn't set.
Set<String>? defineList(String value) => value.isEmpty
    ? null
    : {
        for (final item in value.split(','))
          if (item.trim().isNotEmpty) item.trim(),
      };
