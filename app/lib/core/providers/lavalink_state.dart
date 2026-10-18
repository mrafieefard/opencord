import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';

/// The Lavalink nodes of one server, while the Music settings page shows
/// them (Phase 3 plan §0.1, §12.8): the server sends changes only while
/// this is watched, and stops when the page closes.
class LavalinkNotifier extends AsyncNotifier<LavalinkNodes> {
  LavalinkNotifier(this.serverKey);

  final String serverKey;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  @override
  Future<LavalinkNodes> build() {
    final repository = _repository;
    ref
      ..onDispose(() {
        unawaited(
          repository
              .lavalinkNodes(serverKey, watch: false)
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
    return repository.lavalinkNodes(serverKey, watch: true);
  }

  void set(LavalinkNodes nodes) => state = AsyncData(nodes);

  /// Runs a change and keeps what the server answered; failures reach the
  /// caller.
  Future<void> change(
    Future<LavalinkNodes> Function(OpencordRepository repository, String server)
    action,
  ) async => set(await action(_repository, serverKey));

  /// A node changed or went away, or a line came for the events.
  void apply(RepoEvent event) {
    final nodes = state.value;
    if (nodes == null) return;
    switch (event) {
      case LavalinkNodeChanged(
        :final nodeId,
        :final node,
        :final channelsPlaying,
      ):
        set(
          node == null
              ? nodes.without(nodeId, channelsPlaying: channelsPlaying)
              : nodes.withNode(node, channelsPlaying: channelsPlaying),
        );
      case LavalinkNodeEventReceived(:final event):
        set(nodes.withEvent(event));
      default:
        break;
    }
  }

  Future<void> retry() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(
      () => _repository.lavalinkNodes(serverKey, watch: true),
    );
  }

  Future<void> _rewatch() async {
    try {
      set(await _repository.lavalinkNodes(serverKey, watch: true));
    } on RepoException {
      // Reconnecting again; the next session tries once more.
    }
  }
}

final lavalinkProvider = AsyncNotifierProvider.autoDispose
    .family<LavalinkNotifier, LavalinkNodes, String>(LavalinkNotifier.new);

/// One node, so a status change rebuilds only its row.
final lavalinkNodeProvider = Provider.autoDispose
    .family<LavalinkNode?, (String, int)>(
      (ref, key) => ref.watch(
        lavalinkProvider(key.$1).select((nodes) => nodes.value?.node(key.$2)),
      ),
    );

/// The connection tests this device ran, by node, until the dialog closes.
class LavalinkTestsNotifier extends Notifier<Map<int, LavalinkTestProgress>> {
  LavalinkTestsNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<int, LavalinkTestProgress> build() => const {};

  /// Starts a node's test; its steps arrive as events.
  Future<void> start(int nodeId) async {
    state = {...state, nodeId: LavalinkTestProgress.started(nodeId)};
    try {
      await ref.read(repositoryProvider).testLavalinkNode(serverKey, nodeId);
    } on RepoException catch (error) {
      state = {
        ...state,
        nodeId: LavalinkTestProgress(
          nodeId: nodeId,
          steps: [
            LavalinkTestStep(
              kind: LavalinkTestStepKind.rest,
              state: LavalinkTestStepState.failed,
              detail: error.message,
            ),
            for (final kind in LavalinkTestStepKind.values.skip(1))
              LavalinkTestStep(
                kind: kind,
                state: LavalinkTestStepState.skipped,
              ),
          ],
          done: true,
        ),
      };
    }
  }

  void progressed(LavalinkTestProgress progress) =>
      state = {...state, progress.nodeId: progress};
}

final lavalinkTestsProvider = NotifierProvider.autoDispose
    .family<LavalinkTestsNotifier, Map<int, LavalinkTestProgress>, String>(
      LavalinkTestsNotifier.new,
    );
