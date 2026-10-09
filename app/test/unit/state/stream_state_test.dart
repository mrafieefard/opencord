import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';

// Screen shares in the app's state (Phase 2 V6): streams per server,
// watching, and the quality the share dialog starts at.

const _server = 'opencord.example:7710';

LiveStream _stream(String key, {int channelId = 5, int viewers = 0}) =>
    LiveStream(key: key, channelId: channelId, userId: 7, viewerCount: viewers);

ProviderContainer _container({
  MemoryKeyValueStore? store,
  OpencordRepository? repository,
}) {
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(store ?? MemoryKeyValueStore()),
      if (repository != null) repositoryProvider.overrideWithValue(repository),
    ],
  );
  addTearDown(container.dispose);
  return container;
}

void main() {
  test('streams come, change and go, and a deleted channel takes its own', () {
    final container = _container();
    final streams = container.read(streamsProvider(_server).notifier);

    streams.apply(StreamStarted(_server, _stream('stream:5:7')));
    streams.apply(StreamStarted(_server, _stream('stream:6:8', channelId: 6)));
    streams.apply(StreamChanged(_server, _stream('stream:5:7', viewers: 2)));
    expect(
      container.read(streamsProvider(_server))['stream:5:7']?.viewerCount,
      2,
    );

    streams.apply(const StreamEnded(_server, 'stream:5:7', 5));
    streams.apply(const ChannelDeleted(_server, 6));

    expect(container.read(streamsProvider(_server)), isEmpty);
  });

  test('the dialog starts at the last choice while the server allows it', () {
    const settings = VoiceSettings(
      screenShareMaxResolution: ScreenShareResolution.p1080,
      screenShareMaxFps: 30,
    );
    const last = ScreenShareQuality(ScreenShareResolution.p720, 15);
    const tooFast = ScreenShareQuality(ScreenShareResolution.p720, 60);

    expect(startingQuality(settings, last), last);
    expect(
      startingQuality(settings, tooFast),
      const ScreenShareQuality(ScreenShareResolution.p1080, 30),
    );
    expect(
      startingQuality(settings, null),
      const ScreenShareQuality(ScreenShareResolution.p1080, 30),
    );
  });

  test('the last quality is kept per server', () {
    final store = MemoryKeyValueStore();
    final first = _container(store: store);
    first
        .read(lastScreenQualityProvider(_server).notifier)
        .set(const ScreenShareQuality(ScreenShareResolution.p1440, 60));

    final later = _container(store: store);

    expect(
      later.read(lastScreenQualityProvider(_server)),
      const ScreenShareQuality(ScreenShareResolution.p1440, 60),
    );
    expect(later.read(lastScreenQualityProvider('other.example:7710')), null);
  });

  test('watching asks the repository and ends with the stream', () async {
    final repository = MockRepository(simulateLife: false);
    final container = _container(repository: repository);
    final watching = container.read(watchingProvider.notifier);

    await watching.watch('stream:5:7');
    expect(container.read(watchingProvider), {'stream:5:7'});
    expect(repository.watched, {'stream:5:7'});

    watching.ended('stream:5:7');
    expect(container.read(watchingProvider), isEmpty);
  });
}
