// The media viewer's window session (D86): one window, reused; closed by
// the app or the system; its mode and size remembered.

import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/native_window.dart';

import '../../support/fake_media_window.dart';

const _photo = Attachment(
  id: 1,
  filename: 'a.png',
  contentType: 'image/png',
  sizeBytes: 10,
  kind: AttachmentKind.image,
);

const _request = MediaViewerRequest(
  serverKey: 'server.example:7710',
  media: [_photo],
  index: 0,
);

void main() {
  late FakeMediaWindow window;
  late MemoryKeyValueStore store;
  late ProviderContainer container;

  setUp(() {
    window = FakeMediaWindow();
    store = MemoryKeyValueStore();
    container = ProviderContainer(
      overrides: [
        mediaWindowServiceProvider.overrideWithValue(window),
        keyValueStoreProvider.overrideWithValue(store),
        // No frames to wait for outside a widget test.
        mediaViewerFrameProvider.overrideWithValue(() async {}),
      ],
    );
    // Listened, as the app's root does: the session follows the runner.
    container.listen(mediaViewerProvider, (_, _) {});
  });
  tearDown(() => container.dispose());

  MediaViewerNotifier viewer() => container.read(mediaViewerProvider.notifier);

  test('opening_without_a_window_answers_false', () async {
    window.nextViewId = null;
    expect(await viewer().open(_request), isFalse);
    expect(container.read(mediaViewerProvider), isNull);
  });

  test('opening_sets_the_session', () async {
    expect(await viewer().open(_request), isTrue);
    final session = container.read(mediaViewerProvider)!;
    expect(session.viewId, 7);
    expect(session.request, _request);
    expect(session.status.mode, MediaWindowMode.fullscreen);
    expect(window.calls, ['open:fullscreen']);
  });

  test('opening_again_reuses_the_window', () async {
    await viewer().open(_request);
    const second = MediaViewerRequest(
      serverKey: 'server.example:7710',
      media: [_photo, _photo],
      index: 1,
    );
    expect(await viewer().open(second), isTrue);
    final session = container.read(mediaViewerProvider)!;
    expect(window.calls, ['open:fullscreen', 'focus']);
    expect(session.request, second);
    expect(session.generation, 2);
  });

  test('a_system_close_closes_only_the_viewer', () async {
    await viewer().open(_request);
    window.requestClose();
    await pumpEventQueue();
    expect(container.read(mediaViewerProvider), isNull);
    expect(window.calls.last, 'close');
  });

  test('closing_removes_the_view_before_the_window', () async {
    await viewer().open(_request);
    MediaViewerSession? atClose = container.read(mediaViewerProvider);
    window.onClose = () => atClose = container.read(mediaViewerProvider);
    await viewer().close();
    expect(window.calls.last, 'close');
    expect(atClose, isNull);
  });

  // Saving waits for half a second of quiet, so these wait for real.
  const quiet = Duration(milliseconds: 600);

  test('the_mode_and_size_are_remembered', () async {
    await viewer().open(_request);
    await viewer().toggleMode();
    expect(window.calls, contains('setMode:normal'));
    window.sendStatus(const MediaWindowStatus(mode: MediaWindowMode.normal));
    window.sendGeometry(const WindowGeometry(width: 900, height: 700));
    await Future<void>.delayed(quiet);
    expect(jsonDecode(store.read(mediaViewerPlacementKey)!), {
      'mode': 'normal',
      'width': 900,
      'height': 700,
    });

    await viewer().close();
    await viewer().open(_request);
    expect(window.calls.last, 'open:normal');
  });

  test('a_maximized_window_keeps_its_normal_size', () async {
    store.write(
      mediaViewerPlacementKey,
      '{"mode":"normal","width":900,"height":700}',
    );
    await viewer().open(_request);
    window.sendStatus(
      const MediaWindowStatus(mode: MediaWindowMode.normal, maximized: true),
    );
    window.sendGeometry(const WindowGeometry(width: 1920, height: 1080));
    await Future<void>.delayed(quiet);
    expect(jsonDecode(store.read(mediaViewerPlacementKey)!)['width'], 900);
  });

  test('a_broken_placement_opens_full_screen', () async {
    store.write(mediaViewerPlacementKey, '{');
    await viewer().open(_request);
    expect(window.calls, ['open:fullscreen']);
  });

  test('opening_while_a_close_is_on_its_way_opens_a_new_window', () async {
    await viewer().open(_request);
    final closing = viewer().close();
    final opened = viewer().open(_request);
    await closing;
    expect(await opened, isTrue);
    expect(window.calls, ['open:fullscreen', 'close', 'open:fullscreen']);
    expect(container.read(mediaViewerProvider), isNotNull);
  });
}
