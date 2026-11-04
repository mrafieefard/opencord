// The app's views (D86): the main window's, and the media viewer's own
// while it is open; the viewer inside the app where there is no window.

import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/app_views.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/chat/attachments/video_tile.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

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

Future<(FakeMediaWindow, ProviderContainer)> _pump(
  WidgetTester tester, {
  int? viewId = 7,
}) async {
  final window = FakeMediaWindow()..nextViewId = viewId;
  final container = ProviderContainer(
    overrides: [
      repositoryProvider.overrideWithValue(
        MockRepository(random: Random(1), simulateLife: false),
      ),
      mediaWindowServiceProvider.overrideWithValue(window),
      keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
    ],
  );
  addTearDown(container.dispose);
  await tester.pumpWidget(
    wrapWithView: false,
    UncontrolledProviderScope(
      container: container,
      child: AppViews(
        // The test's one view is the app's; the viewer's never appears.
        findView: (id) => null,
        app: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              senderName: 'Ana',
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
    ),
  );
  return (window, container);
}

void main() {
  testWidgets('the_app_renders_alone_while_the_viewer_is_closed', (
    tester,
  ) async {
    await _pump(tester);
    expect(find.byType(View), findsOneWidget);
    expect(find.byType(VideoTile), findsOneWidget);
  });

  testWidgets('without_a_view_the_viewer_falls_back_to_the_overlay', (
    tester,
  ) async {
    await _pump(tester, viewId: null);
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();
    final viewer = find.byType(MediaViewer);
    expect(viewer, findsOneWidget);
    expect(ModalRoute.of(tester.element(viewer)), isNotNull);
    expect(tester.widget<MediaViewer>(viewer).senderName, 'Ana');
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('with_a_window_the_viewer_does_not_open_in_the_main_window', (
    tester,
  ) async {
    final (window, container) = await _pump(tester);
    await tester.tap(find.byType(VideoTile));
    await tester.pumpAndSettle();
    expect(find.byType(MediaViewer), findsNothing);
    expect(window.calls, ['open:fullscreen']);
    final session = container.read(mediaViewerProvider)!;
    expect(session.request.senderName, 'Ana');
    expect(session.request.sentAt, DateTime(2026, 1, 1));
  });
}
