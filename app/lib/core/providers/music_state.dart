import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/repository/repository.dart';

/// One server's music settings (Phase 3 plan §16.2), kept current by
/// [MusicSettingsChanged] while something watches them.
class MusicSettingsNotifier extends AsyncNotifier<MusicSettings> {
  MusicSettingsNotifier(this.serverKey);

  final String serverKey;

  @override
  Future<MusicSettings> build() =>
      ref.read(repositoryProvider).musicSettings(serverKey);

  void set(MusicSettings settings) => state = AsyncData(settings);

  /// Saves what differs from the settings now; failures reach the caller.
  Future<void> save(MusicSettings after) async {
    final before = state.value ?? const MusicSettings();
    set(
      await ref
          .read(repositoryProvider)
          .updateMusicSettings(serverKey, before, after),
    );
  }

  Future<void> retry() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(build);
  }
}

final musicSettingsProvider = AsyncNotifierProvider.autoDispose
    .family<MusicSettingsNotifier, MusicSettings, String>(
      MusicSettingsNotifier.new,
    );

/// Members blocked from music, while the Music settings page shows them.
class MusicBlocksNotifier extends AsyncNotifier<List<MusicBlock>> {
  MusicBlocksNotifier(this.serverKey);

  final String serverKey;

  @override
  Future<List<MusicBlock>> build() =>
      ref.read(repositoryProvider).musicBlocks(serverKey);

  /// Blocks or unblocks a member; failures reach the caller.
  Future<void> setBlocked(
    int userId, {
    required bool blocked,
    String reason = '',
  }) async {
    state = AsyncData(
      await ref
          .read(repositoryProvider)
          .blockFromMusic(serverKey, userId, blocked: blocked, reason: reason),
    );
  }
}

final musicBlocksProvider = AsyncNotifierProvider.autoDispose
    .family<MusicBlocksNotifier, List<MusicBlock>, String>(
      MusicBlocksNotifier.new,
    );
