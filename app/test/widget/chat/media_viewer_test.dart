// The media viewer, as Telegram's: in its own window (D86), or as a
// see-through layer over the whole window where there is none, shown and
// gone without animation, that a click outside the media closes.

import 'dart:math';

import 'package:clock/clock.dart';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/chat/attachments/video_tile.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_buttons.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

import '../../support/fake_media_window.dart';
import '../../support/fake_window.dart';

const _video = Attachment(
  id: 701,
  filename: 'clip.mp4',
  contentType: 'video/mp4',
  sizeBytes: 1000000,
  kind: AttachmentKind.video,
  width: 640,
  height: 360,
  durationMs: 15000,
);

const _photo = Attachment(
  id: 702,
  filename: 'photo.png',
  contentType: 'image/png',
  sizeBytes: 1000,
  kind: AttachmentKind.image,
  width: 64,
  height: 64,
);

/// The viewer in its own window, as the viewer's view shows it, with a
/// session open so the window's controls have something to act on.
Future<(FakeMediaWindow, List<int>)> _pumpWindow(
  WidgetTester tester, {
  MediaWindowMode mode = MediaWindowMode.fullscreen,
  // A photo first: nothing plays, so nothing is left running.
  List<Attachment> media = const [_photo, _video],
  String? senderName,
  DateTime? sentAt,
  ButtonLayout layout = ButtonLayout.gnomeDefault,
}) async {
  final window = FakeMediaWindow();
  final closes = <int>[];
  final container = ProviderContainer(
    overrides: [
      repositoryProvider.overrideWithValue(
        MockRepository(random: Random(1), simulateLife: false),
      ),
      mediaWindowServiceProvider.overrideWithValue(window),
      keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
      mediaViewerFrameProvider.overrideWithValue(() async {}),
      windowInfoProvider.overrideWithValue(WindowInfo(buttonLayout: layout)),
    ],
  );
  addTearDown(container.dispose);
  container.listen(mediaViewerProvider, (_, _) {});
  await container
      .read(mediaViewerProvider.notifier)
      .open(
        MediaViewerRequest(
          serverKey: 'server.example:7710',
          media: media,
          index: 0,
        ),
      );
  window.calls.clear();
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: MediaViewer(
          serverKey: 'server.example:7710',
          media: media,
          initialIndex: 0,
          senderName: senderName,
          sentAt: sentAt,
          inWindow: true,
          mode: mode,
          onClose: () => closes.add(1),
        ),
      ),
    ),
  );
  // The photo's bytes come after the mock's latency.
  await tester.pump(const Duration(milliseconds: 100));
  return (window, closes);
}

/// The app draws the window's frame on Linux, so the viewer shows the
/// window's buttons there.
final _linux = TargetPlatformVariant.only(TargetPlatform.linux);

Future<(MockRepository, FakeNativeWindow)> _pump(WidgetTester tester) async {
  final repository = MockRepository(random: Random(1), simulateLife: false);
  final window = FakeNativeWindow();
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        repositoryProvider.overrideWithValue(repository),
        nativeWindowProvider.overrideWithValue(window),
        windowInfoProvider.overrideWithValue(
          const WindowInfo(chrome: WindowChrome.custom),
        ),
      ],
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(
          body: AttachmentsView(
            serverKey: 'server.example:7710',
            message: Message(
              id: 1,
              channelId: 10,
              authorId: 100,
              content: '',
              createdAt: DateTime(2026, 1, 1),
              attachments: const [_video],
            ),
          ),
        ),
      ),
    ),
  );
  return (repository, window);
}

void main() {
  testWidgets('the viewer opens and closes without animation', (tester) async {
    await _pump(tester);
    await tester.tap(find.byType(VideoTile));
    await tester.pump();
    await tester.pump();
    final route = ModalRoute.of(tester.element(find.byType(MediaViewer)))!;
    expect(route.transitionDuration, Duration.zero);
    expect(route.reverseTransitionDuration, Duration.zero);
    expect(
      find.ancestor(
        of: find.byType(MediaViewer),
        matching: find.byType(FadeTransition),
      ),
      findsNothing,
    );
  }, variant: _linux);

  testWidgets('a click outside the media closes the viewer, on it not', (
    tester,
  ) async {
    // Wide enough that the 16:9 picture leaves room on either side.
    await tester.binding.setSurfaceSize(const Size(1600, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final (repository, _) = await _pump(tester);
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();

    // On the picture: it stays.
    await tester.tapAt(tester.getCenter(find.byType(Texture)));
    await tester.pump();
    expect(find.byType(MediaViewer), findsOneWidget);

    // Beside it: the viewer closes, and the video with it.
    final picture = tester.getRect(find.byType(Texture));
    await tester.tapAt(Offset(picture.left / 2, picture.center.dy));
    await tester.pump();
    expect(find.byType(MediaViewer), findsNothing);
    expect(repository.openMediaForTest, isNull);
  }, variant: _linux);

  testWidgets('the viewer minimizes and maximizes the window, and closes', (
    tester,
  ) async {
    final (_, window) = await _pump(tester);
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Minimize'));
    await tester.tap(find.byTooltip('Maximize'));
    await tester.pump();
    expect(window.calls, containsAll(['minimize', 'toggleMaximize']));
    expect(find.byType(MediaViewer), findsOneWidget);

    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    expect(find.byType(MediaViewer), findsNothing);
    expect(window.calls, isNot(contains('close')), reason: 'the app stays');
  }, variant: _linux);

  test('the_header_counts_the_items', () {
    expect(mediaHeader(_video, 0, 2), 'Video 1 of 2');
    expect(mediaHeader(_photo, 1, 2), 'Photo 2 of 2');
    expect(mediaHeader(_video, 0, 1), 'clip.mp4');
  });

  testWidgets('the_bottom_line_shows_the_sender_and_time', (tester) async {
    await withClock(Clock.fixed(DateTime(2026, 3, 1)), () async {
      await _pumpWindow(
        tester,
        senderName: 'Ana',
        sentAt: DateTime(2026, 1, 1, 9, 5),
      );
      expect(find.text('Photo 1 of 2'), findsOneWidget);
      expect(find.text('Ana · 1 Jan at 09:05'), findsOneWidget);
    });
  });

  testWidgets('controls_fade_after_a_second_without_movement', (tester) async {
    await _pumpWindow(tester);
    double opacity() => tester
        .widget<AnimatedOpacity>(
          find.ancestor(
            of: find.byTooltip('Download'),
            matching: find.byType(AnimatedOpacity),
          ),
        )
        .opacity;
    expect(opacity(), 1);

    await tester.pump(const Duration(milliseconds: 1100));
    await tester.pump(const Duration(milliseconds: 600));
    expect(opacity(), 0);

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: const Offset(400, 300));
    await mouse.moveTo(const Offset(410, 300));
    await tester.pump(const Duration(milliseconds: 200));
    expect(opacity(), 1);
  });

  /// Taps beside the 16:9 video, on a surface wide enough to leave room.
  Future<void> tapBeside(WidgetTester tester) async {
    final picture = tester.getRect(find.byType(Texture));
    await tester.tapAt(Offset(picture.left / 2, picture.center.dy));
    await tester.pump();
  }

  /// Unmounts the viewer, which stops its video, and lets it stop.
  Future<void> closeViewer(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 1));
  }

  testWidgets('in_the_normal_window_a_click_beside_does_not_close', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1600, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final (_, closes) = await _pumpWindow(
      tester,
      mode: MediaWindowMode.normal,
      media: const [_video],
    );
    await tester.pumpAndSettle();
    await tapBeside(tester);
    expect(closes, isEmpty);
    await closeViewer(tester);
  });

  testWidgets('in_full_screen_a_click_beside_closes', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1600, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final (_, closes) = await _pumpWindow(tester, media: const [_video]);
    await tester.pumpAndSettle();
    await tapBeside(tester);
    expect(closes, [1]);
    await closeViewer(tester);
  });

  testWidgets('alt_enter_switches_the_window_mode', (tester) async {
    final (window, _) = await _pumpWindow(tester);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pump();
    expect(window.calls.last, 'setMode:normal');
    // The placement is saved after half a second of quiet.
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets(
    'windows_buttons_are_windows_caption_buttons',
    (tester) async {
      final (window, closes) = await _pumpWindow(tester);
      expect(find.text(''), findsOneWidget);
      // Full screen: maximize shows restore.
      expect(find.text(''), findsOneWidget);
      expect(find.text(''), findsOneWidget);
      expect(find.byType(GnomeWindowButton), findsNothing);

      await tester.tap(find.text(''));
      await tester.tap(find.text(''));
      await tester.pump();
      // A normal window now: maximize shows maximize.
      expect(find.text(''), findsOneWidget);
      await tester.tap(find.text(''));
      await tester.pump();
      expect(window.calls, ['minimize', 'setMode:normal']);
      expect(closes, [1]);
      await tester.pump(const Duration(seconds: 1));
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );

  testWidgets('linux_buttons_follow_gnomes_layout', (tester) async {
    await _pumpWindow(tester);
    // GNOME's own layout: Close only, on the right.
    expect(find.byType(GnomeWindowButton), findsOneWidget);
    expect(find.text(''), findsNothing);
  }, variant: _linux);

  testWidgets('linux_buttons_follow_the_desktops_layout', (tester) async {
    await _pumpWindow(
      tester,
      layout: ButtonLayout.parse('close,minimize,maximize:'),
    );
    final buttons = find.byType(GnomeWindowButton);
    expect(buttons, findsNWidgets(3));
    expect(tester.getTopLeft(buttons.first).dx, lessThan(100));
  }, variant: _linux);

  testWidgets(
    'macos_draws_no_window_buttons',
    (tester) async {
      await _pumpWindow(tester);
      // Both ends of the top edge, each drawing nothing.
      expect(find.byType(MediaWindowButtons), findsNWidgets(2));
      expect(find.byType(GnomeWindowButton), findsNothing);
      expect(find.text(''), findsNothing);
    },
    variant: TargetPlatformVariant.only(TargetPlatform.macOS),
  );

  testWidgets('a_normal_window_on_linux_resizes_from_its_edges', (
    tester,
  ) async {
    final (window, _) = await _pumpWindow(tester, mode: MediaWindowMode.normal);
    final size = tester.view.physicalSize / tester.view.devicePixelRatio;
    final press = await tester.startGesture(Offset(2, size.height / 2));
    await press.up();
    await tester.pump();
    expect(window.calls, contains('startResize:left'));
  }, variant: _linux);

  testWidgets('full_screen_has_no_resize_edges', (tester) async {
    final (window, _) = await _pumpWindow(tester);
    final size = tester.view.physicalSize / tester.view.devicePixelRatio;
    final press = await tester.startGesture(Offset(2, size.height / 2));
    await press.up();
    await tester.pump();
    expect(
      window.calls.where((call) => call.startsWith('startResize')),
      isEmpty,
    );
  }, variant: _linux);

  testWidgets('a_cancelled_press_lets_the_controls_fade', (tester) async {
    await _pumpWindow(tester);
    final press = await tester.startGesture(const Offset(400, 300));
    await press.cancel();
    await tester.pump(const Duration(milliseconds: 1100));
    await tester.pump(const Duration(milliseconds: 600));
    final opacity = tester
        .widget<AnimatedOpacity>(
          find.ancestor(
            of: find.byTooltip('Download'),
            matching: find.byType(AnimatedOpacity),
          ),
        )
        .opacity;
    expect(opacity, 0);
  });

  testWidgets(
    'macos_leaves_the_title_strips_double_click_to_the_system',
    (tester) async {
      final (window, _) = await _pumpWindow(tester);
      await tester.tapAt(const Offset(400, 10));
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tapAt(const Offset(400, 10));
      await tester.pump(const Duration(seconds: 1));
      expect(window.calls, isNot(contains('setMode:normal')));
    },
    variant: TargetPlatformVariant.only(TargetPlatform.macOS),
  );

  group('zoom, as Telegram\'s, across the whole screen', () {
    // The 16:9 video fits 1052×592 between the top and bottom rows.
    Future<(FakeMediaWindow, List<int>)> pumpVideo(
      WidgetTester tester, {
      MediaWindowMode mode = MediaWindowMode.fullscreen,
    }) async {
      await tester.binding.setSurfaceSize(const Size(1600, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final result = await _pumpWindow(
        tester,
        mode: mode,
        media: const [_video],
      );
      await tester.pumpAndSettle();
      return result;
    }

    /// Turns the mouse wheel at [at]: up zooms in.
    Future<void> wheel(WidgetTester tester, Offset at, double dy) async {
      final mouse = TestPointer(1, PointerDeviceKind.mouse);
      await tester.sendEventToBinding(mouse.hover(at));
      await tester.sendEventToBinding(mouse.scroll(Offset(0, dy)));
      await tester.sendEventToBinding(mouse.removePointer());
      await tester.pumpAndSettle();
    }

    Rect picture(WidgetTester tester) => tester.getRect(find.byType(Texture));

    testWidgets('the_zoom_has_the_whole_screen_not_the_media_box', (
      tester,
    ) async {
      await pumpVideo(tester);
      expect(
        tester.getSize(find.byType(InteractiveViewer)),
        const Size(1600, 700),
      );
      await closeViewer(tester);
    });

    testWidgets('a_zoomed_picture_narrower_than_the_screen_stays_centred', (
      tester,
    ) async {
      await pumpVideo(tester);
      final before = picture(tester);
      // Near its left edge: the zoom would grow it to the right.
      await wheel(tester, Offset(before.left + 20, before.center.dy), -60);

      final after = picture(tester);
      expect(after.width, greaterThan(before.width));
      expect(after.width, lessThan(1600));
      expect(after.center.dx, closeTo(800, 0.5));
      // Taller than the screen: no gap above or below.
      expect(after.top, lessThanOrEqualTo(0.5));
      expect(after.bottom, greaterThanOrEqualTo(699.5));
      await closeViewer(tester);
    });

    testWidgets('a_zoomed_picture_pans_until_its_edge_meets_the_screens', (
      tester,
    ) async {
      await pumpVideo(tester);
      await wheel(tester, const Offset(800, 350), -200);
      expect(picture(tester).width, greaterThan(1600));

      await tester.dragFrom(const Offset(800, 350), const Offset(3000, 0));
      await tester.pumpAndSettle();
      expect(picture(tester).left, closeTo(0, 0.5));

      await tester.dragFrom(const Offset(800, 350), const Offset(-6000, 0));
      await tester.pumpAndSettle();
      expect(picture(tester).right, closeTo(1600, 0.5));
      await closeViewer(tester);
    });

    testWidgets('the_videos_controls_stay_put_while_its_picture_zooms', (
      tester,
    ) async {
      await pumpVideo(tester);
      final bar = tester.getRect(find.byType(Slider).first);
      await wheel(tester, const Offset(800, 350), -200);
      expect(tester.getRect(find.byType(Slider).first), bar);
      await closeViewer(tester);
    });

    testWidgets('a_click_where_the_zoomed_picture_now_is_does_not_close', (
      tester,
    ) async {
      final (_, closes) = await pumpVideo(tester);
      final before = picture(tester);
      await wheel(tester, const Offset(800, 350), -200);
      // Beside the picture before the zoom, on it after.
      await tester.tapAt(Offset(before.left / 2, 350));
      await tester.pump();
      expect(closes, isEmpty);
      await closeViewer(tester);
    });

    testWidgets('beside_the_picture_a_normal_window_still_drags', (
      tester,
    ) async {
      final (window, _) = await pumpVideo(tester, mode: MediaWindowMode.normal);
      final at = Offset(picture(tester).left / 2, 350);
      await tester.dragFrom(at, const Offset(60, 0));
      await tester.pump();
      expect(window.calls, contains('startDrag'));
      await closeViewer(tester);
    });
  });

  group('rotate, beside Download', () {
    Future<(FakeMediaWindow, List<int>)> pumpVideo(WidgetTester tester) async {
      await tester.binding.setSurfaceSize(const Size(1600, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final result = await _pumpWindow(tester, media: const [_video]);
      await tester.pumpAndSettle();
      return result;
    }

    int turns(WidgetTester tester) =>
        tester.widget<RotatedBox>(find.byType(RotatedBox)).quarterTurns;

    testWidgets('rotate_turns_the_picture_a_quarter_left', (tester) async {
      await pumpVideo(tester);
      expect(
        tester.getTopLeft(find.byTooltip('Rotate')).dx,
        greaterThan(tester.getTopLeft(find.byTooltip('Download')).dx),
      );
      await tester.tap(find.byTooltip('Rotate'));
      await tester.pumpAndSettle();
      final picture = tester.getRect(find.byType(Texture));
      expect(picture.height, greaterThan(picture.width));
      expect(turns(tester), 3, reason: 'counter-clockwise, as Telegram\'s');
      await closeViewer(tester);
    });

    testWidgets('rotate_resets_with_the_next_item', (tester) async {
      await _pumpWindow(tester);
      await tester.tap(find.byTooltip('Rotate'));
      await tester.pump();
      expect(turns(tester), 3);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.pumpAndSettle();
      expect(turns(tester), 0);
      await closeViewer(tester);
    });

    testWidgets('clicks_beside_follow_the_turned_picture', (tester) async {
      // Tall: the turned video reaches where the unturned one wasn't.
      await tester.binding.setSurfaceSize(const Size(700, 1600));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final (_, closes) = await _pumpWindow(tester, media: const [_video]);
      await tester.pumpAndSettle();
      final before = tester.getRect(find.byType(Texture));
      await tester.tap(find.byTooltip('Rotate'));
      await tester.pumpAndSettle();
      final turned = tester.getRect(find.byType(Texture));
      expect(turned.top, lessThan(before.top));

      // On the turned picture, above where the unturned one was.
      await tester.tapAt(Offset(350, (turned.top + before.top) / 2));
      await tester.pump();
      expect(closes, isEmpty);
      // Below the turned picture.
      await tester.tapAt(Offset(350, (turned.bottom + 1546) / 2));
      await tester.pump();
      expect(closes, [1]);
      await closeViewer(tester);
    });
  });

  group("the video's bar, as Telegram's", () {
    Future<(FakeMediaWindow, List<int>)> pumpVideo(WidgetTester tester) async {
      await tester.binding.setSurfaceSize(const Size(1600, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final result = await _pumpWindow(tester, media: const [_video]);
      await tester.pumpAndSettle();
      return result;
    }

    final bar = find.byKey(const ValueKey('video-bar'));

    testWidgets('the_video_bar_is_telegrams_size', (tester) async {
      await pumpVideo(tester);
      final rect = tester.getRect(bar);
      expect(rect.size, const Size(480, 70));
      expect(rect.center.dx, 800);
      expect(rect.bottom, 700 - 7);
      await closeViewer(tester);
    });

    testWidgets('the_video_keeps_clear_of_the_bar', (tester) async {
      await pumpVideo(tester);
      final picture = tester.getRect(find.byType(Texture));
      expect(picture.bottom, lessThanOrEqualTo(tester.getRect(bar).top));
      expect(picture.center.dy, 350, reason: 'still centred');
      await closeViewer(tester);
    });

    testWidgets('the_video_bar_fades_with_the_controls', (tester) async {
      await pumpVideo(tester);
      double opacity() => tester
          .widget<AnimatedOpacity>(
            find.ancestor(of: bar, matching: find.byType(AnimatedOpacity)),
          )
          .opacity;
      expect(opacity(), 1);
      await tester.pump(const Duration(milliseconds: 1100));
      await tester.pump(const Duration(milliseconds: 600));
      expect(opacity(), 0);
      await closeViewer(tester);
    });

    testWidgets('the_bar_shows_the_time_played_and_left', (tester) async {
      await pumpVideo(tester);
      final texts = tester
          .widgetList<Text>(
            find.descendant(of: bar, matching: find.byType(Text)),
          )
          .map((text) => text.data ?? '')
          .toList();
      int seconds(String clock) {
        final parts = clock.replaceFirst('-', '').split(':');
        return int.parse(parts[0]) * 60 + int.parse(parts[1]);
      }

      final played = texts.singleWhere(
        (t) => RegExp(r'^\d\d:\d\d$').hasMatch(t),
      );
      final left = texts.singleWhere(
        (t) => RegExp(r'^-\d\d:\d\d$').hasMatch(t),
      );
      // The mock plays a video it doesn't know for 10 s.
      expect(seconds(played) + seconds(left), inInclusiveRange(9, 10));
      await closeViewer(tester);
    });

    testWidgets('speed_cycles_one_to_three_and_back', (tester) async {
      await pumpVideo(tester);
      final repository =
          ProviderScope.containerOf(
                tester.element(find.byType(MediaViewer)),
              ).read(repositoryProvider)
              as MockRepository;
      expect(find.descendant(of: bar, matching: find.text('1x')), findsOne);
      for (final label in ['1.5x', '2x', '3x', '1x']) {
        await tester.tap(find.byTooltip('Speed'));
        await tester.pump();
        expect(find.descendant(of: bar, matching: find.text(label)), findsOne);
      }
      expect(repository.mediaSpeeds, [1.5, 2, 3, 1]);
      await closeViewer(tester);
    });

    testWidgets('the_bottom_left_text_keeps_left_of_the_bar', (tester) async {
      await pumpVideo(tester);
      final text = tester.getRect(find.text('clip.mp4'));
      expect(text.right, lessThanOrEqualTo(tester.getRect(bar).left));
      await closeViewer(tester);
    });
  });

  group("the video's full screen", () {
    Future<(FakeMediaWindow, List<int>)> pumpVideo(
      WidgetTester tester, {
      MediaWindowMode mode = MediaWindowMode.fullscreen,
    }) async {
      await tester.binding.setSurfaceSize(const Size(1600, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final result = await _pumpWindow(
        tester,
        media: const [_video],
        mode: mode,
      );
      await tester.pumpAndSettle();
      return result;
    }

    testWidgets('full_screen_fills_the_window_with_the_video', (tester) async {
      await pumpVideo(tester);
      final before = tester.getRect(find.byType(Texture));
      await tester.tap(find.byTooltip('Full screen'));
      await tester.pumpAndSettle();
      final picture = tester.getRect(find.byType(Texture));
      expect(picture.height, 700);
      expect(
        picture.width / picture.height,
        closeTo(before.width / before.height, 0.01),
      );
      // Only the bar and the window's buttons are left.
      expect(find.text('clip.mp4'), findsNothing);
      expect(find.byTooltip('Download'), findsNothing);
      expect(find.byTooltip('Rotate'), findsNothing);
      expect(find.byType(MediaWindowButtons), findsWidgets);
      await closeViewer(tester);
    });

    testWidgets('esc_ends_full_screen_before_closing', (tester) async {
      final (_, closes) = await pumpVideo(tester);
      await tester.tap(find.byTooltip('Full screen'));
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(closes, isEmpty);
      expect(find.text('clip.mp4'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      expect(closes, [1]);
      await closeViewer(tester);
    });

    testWidgets('full_screen_from_a_normal_window_comes_back_normal', (
      tester,
    ) async {
      final (window, _) = await pumpVideo(tester, mode: MediaWindowMode.normal);
      await tester.tap(find.byTooltip('Full screen'));
      await tester.pumpAndSettle();
      expect(window.calls, contains('setMode:fullscreen'));
      await tester.tap(find.byTooltip('Exit full screen'));
      await tester.pumpAndSettle();
      expect(window.calls.last, 'setMode:normal');
      await closeViewer(tester);
    });
  });
}
