import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';

/// Domains & network for one server, while the owner's settings page shows
/// it (Phase 3 plan §0.1): the server sends changes only while this is
/// watched, and stops when the page closes.
class NetworkNotifier extends AsyncNotifier<NetworkInfo> {
  NetworkNotifier(this.serverKey);

  final String serverKey;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  @override
  Future<NetworkInfo> build() {
    final repository = _repository;
    ref
      ..onDispose(() {
        unawaited(
          repository
              .watchNetwork(serverKey, watch: false)
              .then<void>((_) {}, onError: (Object _) {}),
        );
      })
      // A new session starts unwatched: watch again.
      ..listen(serverProvider(serverKey).select((s) => s.epoch), (
        previous,
        next,
      ) {
        if (previous != null && previous != next) unawaited(_rewatch());
      });
    return repository.watchNetwork(serverKey, watch: true);
  }

  /// The network as the server last described it.
  void set(NetworkInfo info) => state = AsyncData(info);

  /// Runs a change and keeps what the server answered; failures reach the
  /// caller.
  Future<void> change(
    Future<NetworkInfo> Function(OpencordRepository repository, String server)
    action,
  ) async => set(await action(_repository, serverKey));

  Future<void> retry() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(
      () => _repository.watchNetwork(serverKey, watch: true),
    );
  }

  Future<void> _rewatch() async {
    try {
      set(await _repository.watchNetwork(serverKey, watch: true));
    } on RepoException {
      // Reconnecting again; the next session tries once more.
    }
  }
}

final networkProvider = AsyncNotifierProvider.autoDispose
    .family<NetworkNotifier, NetworkInfo, String>(NetworkNotifier.new);
