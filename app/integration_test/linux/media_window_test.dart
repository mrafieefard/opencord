import 'dart:math';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/app_views.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_service.dart';

// The media viewer's own window on the Linux runner (D86): a second view
// on the app's engine in a window of its own, full screen and see-through,
// that switches to a normal window and closes without closing the app.
//
//   flutter test integration_test/linux/media_window_test.dart -d linux
//
// It needs a desktop session. Keep the mouse off the screen's top left:
// the test binding fails on a real pointer over a view it doesn't know.
// --dart-define=OPENCORD_HOLD=<seconds> keeps each mode open that long,
// to look at the window or ask KWin about it.

const _photo = Attachment(
  id: 1,
  filename: 'photo.png',
  contentType: 'image/png',
  sizeBytes: 1000,
  kind: AttachmentKind.image,
  width: 64,
  height: 64,
);

Future<void> _until(WidgetTester tester, bool Function() done) async {
  for (var i = 0; i < 100 && !done(); i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
  expect(done(), isTrue);
}

/// Keeps the window open for OPENCORD_HOLD seconds, to look at it.
Future<void> _hold() async {
  const hold = int.fromEnvironment('OPENCORD_HOLD');
  if (hold > 0) await Future<void>.delayed(const Duration(seconds: hold));
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the_viewer_opens_its_own_window_and_closes_it', (tester) async {
    final window = ChannelMediaWindow();
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
        child: const AppViews(
          app: MaterialApp(home: Scaffold(body: Text('The app'))),
        ),
      ),
    );
    final statuses = <MediaWindowStatus>[];
    window.status.listen(statuses.add);

    final opened = await container
        .read(mediaViewerProvider.notifier)
        .open(
          const MediaViewerRequest(
            serverKey: 'server.example:7710',
            media: [_photo],
            index: 0,
            senderName: 'Ana',
          ),
        );
    expect(opened, isTrue);
    await _until(tester, () => PlatformDispatcher.instance.views.length == 2);
    await _until(tester, () => find.byType(MediaViewer).evaluate().isNotEmpty);
    expect(find.text('Ana'), findsOneWidget);
    await _until(
      tester,
      () => statuses.any((s) => s.mode == MediaWindowMode.fullscreen),
    );

    await _hold();
    await container.read(mediaViewerProvider.notifier).toggleMode();
    await _until(
      tester,
      () => statuses.isNotEmpty && statuses.last.mode == MediaWindowMode.normal,
    );
    await _hold();
    // Closing waits for the frame without the view, which the test draws.
    final closed = container.read(mediaViewerProvider.notifier).close();
    await _until(tester, () => PlatformDispatcher.instance.views.length == 1);
    await closed;
    expect(find.text('The app'), findsOneWidget);
  });
}
