// Picture-in-picture (D87): the viewer's window turned small, the same
// video playing on, with its own controls; back to the viewer as it was.

import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/app_views.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/attachments/video_player_view.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_service.dart';

import '../../support/fake_media_window.dart';

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

const _request = MediaViewerRequest(
  serverKey: 'server.example:7710',
  media: [_video],
  index: 0,
);

/// The viewer's window as the app shows it, open on the video.
Future<(FakeMediaWindow, ProviderContainer)> _pump(
  WidgetTester tester, {
  String? placement,
}) async {
  final window = FakeMediaWindow();
  final store = MemoryKeyValueStore();
  if (placement != null) store.write(mediaViewerPlacementKey, placement);
  final container = ProviderContainer(
    overrides: [
      repositoryProvider.overrideWithValue(
        MockRepository(random: Random(1), simulateLife: false),
      ),
      mediaWindowServiceProvider.overrideWithValue(window),
      keyValueStoreProvider.overrideWithValue(store),
      mediaViewerFrameProvider.overrideWithValue(() async {}),
    ],
  );
  addTearDown(container.dispose);
  container.listen(mediaViewerProvider, (_, _) {});
  await container.read(mediaViewerProvider.notifier).open(_request);
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: const MediaViewerWindow(),
    ),
  );
  await tester.pumpAndSettle();
  return (window, container);
}

/// Unmounts the viewer, which stops its video, and lets it stop.
Future<void> _unmount(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
  await tester.pump(const Duration(seconds: 1));
}

void main() {
  testWidgets('pip_keeps_the_same_player', (tester) async {
    final (window, _) = await _pump(tester);
    final player = tester.state(find.byType(VideoPlayerView));

    await tester.tap(find.byTooltip('Picture in picture'));
    await tester.pumpAndSettle();
    expect(window.calls.last, startsWith('pip:'));
    expect(tester.state(find.byType(VideoPlayerView)), same(player));

    await tester.tap(find.byTooltip('Back to the viewer'));
    await tester.pumpAndSettle();
    expect(window.calls.last, 'setMode:fullscreen');
    expect(tester.state(find.byType(VideoPlayerView)), same(player));
    await _unmount(tester);
  });

  testWidgets('pip_shows_its_own_controls', (tester) async {
    await _pump(tester);
    await tester.tap(find.byTooltip('Picture in picture'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Back to the viewer'), findsOneWidget);
    expect(find.byTooltip('Close'), findsOneWidget);
    expect(find.byTooltip('Pause'), findsOneWidget);
    expect(find.byTooltip('Mute'), findsOneWidget);
    expect(find.byTooltip('Download'), findsNothing);
    expect(find.byKey(const ValueKey('video-bar')), findsNothing);
    await _unmount(tester);
  });

  testWidgets('pip_follows_the_videos_shape', (tester) async {
    final (window, _) = await _pump(tester);
    await tester.tap(find.byTooltip('Picture in picture'));
    await tester.pumpAndSettle();
    // The mock's video of unknown size is 4:3.
    expect(window.calls.last, 'pip:1.33');
    await _unmount(tester);
  });

  testWidgets('closing_pip_closes_the_window', (tester) async {
    final (window, container) = await _pump(tester);
    await tester.tap(find.byTooltip('Picture in picture'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Close'));
    await tester.pumpAndSettle();
    expect(container.read(mediaViewerProvider), isNull);
    expect(window.calls.last, 'close');
    await _unmount(tester);
  });

  testWidgets('back_returns_to_a_normal_window', (tester) async {
    final (window, _) = await _pump(tester, placement: '{"mode":"normal"}');
    await tester.tap(find.byTooltip('Picture in picture'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Back to the viewer'));
    await tester.pumpAndSettle();
    expect(window.calls.last, 'setMode:normal');
    await _unmount(tester);
  });

  testWidgets('photos_offer_no_pip', (tester) async {
    final (_, container) = await _pump(tester);
    await container
        .read(mediaViewerProvider.notifier)
        .open(
          const MediaViewerRequest(
            serverKey: 'server.example:7710',
            media: [_photo],
            index: 0,
          ),
        );
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byTooltip('Picture in picture'), findsNothing);
    await _unmount(tester);
  });

  testWidgets('a_drag_moves_pip_and_a_double_click_goes_back', (tester) async {
    final (window, _) = await _pump(tester);
    await tester.tap(find.byTooltip('Picture in picture'));
    await tester.pumpAndSettle();
    final size = tester.view.physicalSize / tester.view.devicePixelRatio;
    // Between the play button and the times: the video itself.
    final on = Offset(size.width / 4, size.height * 0.6);
    await tester.dragFrom(on, const Offset(40, 0));
    await tester.pump();
    expect(window.calls, contains('startDrag'));

    await tester.tapAt(on);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(on);
    await tester.pumpAndSettle();
    expect(window.calls.last, 'setMode:fullscreen');
    await _unmount(tester);
  });
}
