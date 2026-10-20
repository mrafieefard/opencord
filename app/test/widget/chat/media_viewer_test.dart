// The media viewer, as Telegram's: a see-through layer over the whole
// window, shown and gone without animation, that a click outside the media
// closes, with the window's own minimize and maximize beside its close.

import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/chat/attachments/video_tile.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

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
}
