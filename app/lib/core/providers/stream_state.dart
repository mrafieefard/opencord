import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';

/// Screen shares (Phase 2 V6): every stream on a server, the ones this
/// device watches, and its own.
class StreamsNotifier extends Notifier<Map<String, LiveStream>> {
  StreamsNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<String, LiveStream> build() => const {};

  void apply(RepoEvent event) {
    switch (event) {
      case Ready(:final snapshot):
        state = snapshot.streams;
      case StreamStarted(:final stream) || StreamChanged(:final stream):
        state = {...state, stream.key: stream};
      case StreamEnded(:final key) when state.containsKey(key):
        state = {...state}..remove(key);
      case ChannelDeleted(:final channelId):
        state = {
          for (final MapEntry(:key, :value) in state.entries)
            if (value.channelId != channelId) key: value,
        };
      default:
        break;
    }
  }
}

final streamsProvider =
    NotifierProvider.family<StreamsNotifier, Map<String, LiveStream>, String>(
      StreamsNotifier.new,
    );

/// The streams this device watches in its voice channel (§9.5): nothing
/// of a stream is received until it is here. A new channel starts with
/// none.
class WatchingNotifier extends Notifier<Set<String>> {
  @override
  Set<String> build() {
    ref.watch(
      voiceSessionProvider.select(
        (voice) => (voice.serverKey, voice.channelId),
      ),
    );
    return const {};
  }

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// A full stream fails with [RepoErrorKind.streamFull].
  Future<void> watch(String key) async {
    if (state.contains(key)) return;
    await _repository.watchStream(key);
    state = {...state, key};
  }

  Future<void> unwatch(String key) async {
    if (!state.contains(key)) return;
    state = {...state}..remove(key);
    await _repository.unwatchStream(key);
  }

  /// The stream is over.
  void ended(String key) {
    if (state.contains(key)) state = {...state}..remove(key);
  }
}

final watchingProvider = NotifierProvider<WatchingNotifier, Set<String>>(
  WatchingNotifier.new,
);

/// This device's own screen share while live, and going live.
class OwnScreenShareNotifier extends Notifier<OwnScreenShare?> {
  @override
  OwnScreenShare? build() => null;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// Goes live (or changes the source while live); remembers the quality
  /// for this server. A [RepoException] says why it could not start.
  Future<void> start(
    String serverKey,
    ScreenShareQuality quality, {
    bool audio = true,
  }) async {
    ref.read(lastScreenQualityProvider(serverKey).notifier).set(quality);
    await _repository.startScreenShare(quality, audio: audio);
  }

  Future<void> changeQuality(
    String serverKey,
    ScreenShareQuality quality, {
    bool audio = true,
  }) async {
    ref.read(lastScreenQualityProvider(serverKey).notifier).set(quality);
    await _repository.updateScreenShare(quality, audio: audio);
  }

  Future<void> stop() => _repository.stopScreenShare();

  /// The repository's news: live, changed, or over.
  void set(OwnScreenShare? share) => state = share;
}

final ownScreenShareProvider =
    NotifierProvider<OwnScreenShareNotifier, OwnScreenShare?>(
      OwnScreenShareNotifier.new,
    );

/// Who watches this device's own stream; only the streamer hears it.
class OwnStreamViewersNotifier extends Notifier<List<int>> {
  @override
  List<int> build() {
    ref.watch(ownScreenShareProvider.select((share) => share?.key));
    return const [];
  }

  void apply(StreamViewersChanged event) {
    if (event.key != ref.read(ownScreenShareProvider)?.key) return;
    if (!listEquals(event.viewerIds, state)) {
      state = List.unmodifiable(event.viewerIds);
    }
  }
}

final ownStreamViewersProvider =
    NotifierProvider<OwnStreamViewersNotifier, List<int>>(
      OwnStreamViewersNotifier.new,
    );

/// The quality last chosen on a server (§9.1); null when never chosen.
class LastScreenQualityNotifier extends Notifier<ScreenShareQuality?> {
  LastScreenQualityNotifier(this.serverKey);

  final String serverKey;

  String get _key => 'screenQuality.$serverKey';

  @override
  ScreenShareQuality? build() {
    final saved = ref.watch(keyValueStoreProvider).read(_key);
    final parts = saved?.split('@');
    if (parts == null || parts.length != 2) return null;
    final resolution = ScreenShareResolution.values
        .where((resolution) => resolution.name == parts[0])
        .firstOrNull;
    final fps = int.tryParse(parts[1]);
    if (resolution == null || !ScreenShareQuality.frameRates.contains(fps)) {
      return null;
    }
    return ScreenShareQuality(resolution, fps!);
  }

  void set(ScreenShareQuality quality) {
    state = quality;
    ref
        .read(keyValueStoreProvider)
        .write(_key, '${quality.resolution.name}@${quality.fps}');
  }
}

final lastScreenQualityProvider =
    NotifierProvider.family<
      LastScreenQualityNotifier,
      ScreenShareQuality?,
      String
    >(LastScreenQualityNotifier.new);

/// What the share dialog starts at (§9.1): the last choice on this server
/// while the server still allows it, else the server's maximum.
ScreenShareQuality startingQuality(
  VoiceSettings settings,
  ScreenShareQuality? last,
) {
  final maximum = ScreenShareQuality(
    settings.screenShareMaxResolution,
    settings.screenShareMaxFps,
  );
  if (last != null &&
      last.within(
        settings.screenShareMaxResolution,
        settings.screenShareMaxFps,
      )) {
    return last;
  }
  return maximum;
}
