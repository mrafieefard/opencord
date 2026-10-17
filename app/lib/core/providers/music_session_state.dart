import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/providers/music_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

/// A channel on a server, as the music providers key it.
typedef MusicChannel = ({String server, int channel});

/// One voice channel's player (Phase 3 plan §14.3), while something shows
/// it: subscribed when first watched, kept current by
/// [MusicSessionChanged], unsubscribed when nothing watches any more.
/// Older snapshots than the one at hand are dropped.
class MusicSessionNotifier extends AsyncNotifier<MusicSession> {
  MusicSessionNotifier(this.channel);

  final MusicChannel channel;

  @override
  Future<MusicSession> build() async {
    final repository = ref.read(repositoryProvider);
    ref.onDispose(
      () => unawaited(
        repository
            .musicUnsubscribe(channel.server, channel.channel)
            .catchError((Object _) {}),
      ),
    );
    return repository.musicSubscribe(channel.server, channel.channel);
  }

  /// A snapshot from an event or an answer; ignored when older.
  void apply(MusicSession session) {
    final current = state.value;
    if (current != null && session.version < current.version) return;
    state = AsyncData(session);
  }

  Future<void> retry() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(build);
  }
}

final musicSessionProvider = AsyncNotifierProvider.autoDispose
    .family<MusicSessionNotifier, MusicSession, MusicChannel>(
      MusicSessionNotifier.new,
    );

/// What plays in each voice channel of a server, for the now-playing card
/// (§19.3); channels with nothing playing are absent.
class MusicNowPlayingNotifier extends Notifier<Map<int, MusicNowPlaying>> {
  MusicNowPlayingNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<int, MusicNowPlaying> build() => const {};

  void apply(int channelId, MusicNowPlaying? nowPlaying) {
    if (state[channelId] == nowPlaying) return;
    state = {
      for (final entry in state.entries)
        if (entry.key != channelId) entry.key: entry.value,
      channelId: ?nowPlaying,
    };
  }
}

final musicNowPlayingProvider =
    NotifierProvider.family<
      MusicNowPlayingNotifier,
      Map<int, MusicNowPlaying>,
      String
    >(MusicNowPlayingNotifier.new);

/// The last few activity lines of each channel's music panel (§14.4).
class MusicNoticesNotifier extends Notifier<List<MusicNotice>> {
  MusicNoticesNotifier(this.channel);

  final MusicChannel channel;

  static const kept = 20;

  @override
  List<MusicNotice> build() => const [];

  void add(MusicNotice notice) => state = [notice, ...state.take(kept - 1)];
}

final musicNoticesProvider =
    NotifierProvider.family<
      MusicNoticesNotifier,
      List<MusicNotice>,
      MusicChannel
    >(MusicNoticesNotifier.new);

/// Routes the music events to the providers above.
void routeMusicEvent(Ref ref, RepoEvent event) {
  switch (event) {
    case MusicSessionChanged(:final serverKey, :final session):
      final provider = musicSessionProvider((
        server: serverKey,
        channel: session.channelId,
      ));
      if (ref.exists(provider)) ref.read(provider.notifier).apply(session);
    case MusicNowPlayingChanged(
      :final serverKey,
      :final channelId,
      :final nowPlaying,
    ):
      ref
          .read(musicNowPlayingProvider(serverKey).notifier)
          .apply(channelId, nowPlaying);
    case MusicNoticeReceived(:final serverKey, :final notice):
      ref
          .read(
            musicNoticesProvider((
              server: serverKey,
              channel: notice.channelId,
            )).notifier,
          )
          .add(notice);
    default:
      break;
  }
}

/// Whether music is set up on a server (§19.1): its settings say so.
/// Watching it keeps the settings loaded and current.
final musicConfiguredProvider = Provider.family<bool, String>(
  (ref, serverKey) =>
      ref.watch(
        musicSettingsProvider(
          serverKey,
        ).select((settings) => settings.value?.configured),
      ) ??
      false,
);

const musicListeningKey = 'ui.music';

/// This person's music volume, mute and ducking, for every server (§19.8);
/// kept on this device and given to voice media.
class MusicListeningNotifier extends Notifier<MusicListening> {
  @override
  MusicListening build() {
    final json = readJson(ref.watch(keyValueStoreProvider), musicListeningKey);
    if (json is! Map) return const MusicListening();
    final volume = json['volume'];
    return MusicListening(
      volume: volume is int ? volume.clamp(0, 200) : 100,
      muted: json['muted'] == true,
      duckWhileTalking: json['duck'] == true,
    );
  }

  void update(MusicListening Function(MusicListening listening) change) {
    state = change(state);
    ref
        .read(keyValueStoreProvider)
        .write(
          musicListeningKey,
          jsonEncode({
            'volume': state.volume,
            'muted': state.muted,
            'duck': state.duckWhileTalking,
          }),
        );
  }
}

final musicListeningProvider =
    NotifierProvider<MusicListeningNotifier, MusicListening>(
      MusicListeningNotifier.new,
    );

/// What the right-side panel shows next to a voice channel (§19.2).
enum RightPanelMode { people, music }

/// The right-side panel's mode, for the whole window; Music falls back to
/// People where there is no music to show.
class RightPanelModeNotifier extends Notifier<RightPanelMode> {
  @override
  RightPanelMode build() => RightPanelMode.people;

  void set(RightPanelMode mode) => state = mode;

  /// The Music buttons: open the music panel, or back to the people.
  void toggleMusic() => state = state == RightPanelMode.music
      ? RightPanelMode.people
      : RightPanelMode.music;
}

final rightPanelModeProvider =
    NotifierProvider<RightPanelModeNotifier, RightPanelMode>(
      RightPanelModeNotifier.new,
    );

/// The music panel shows in the voice view's main area (§19.7).
class MusicExpandedNotifier extends Notifier<bool> {
  @override
  bool build() => false;

  void set(bool expanded) => state = expanded;
}

final musicExpandedProvider = NotifierProvider<MusicExpandedNotifier, bool>(
  MusicExpandedNotifier.new,
);

/// Sources members can search on a server now: enabled and offered.
List<String> searchableSources(MusicSettings settings) => [
  for (final source in musicSources)
    if (settings.availableSources.contains(source.key) &&
        settings.allows(source.key))
      source.key,
];
