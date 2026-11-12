import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';

// The activity sessions a server runs (plugins M3 Task 9). Every event goes
// down the app's road: repository events → event pump → the provider.

final _base = DateTime(2026, 11, 12, 14);
const _otherServer = 'other.example:7710';

ActivitySession _session(
  int id, {
  int channelId = 20,
  List<int> viewers = const [],
}) => ActivitySession(
  id: id,
  channelId: channelId,
  pluginId: 'paint',
  activityKey: 'board',
  name: 'Paint together',
  icon: '',
  pluginName: 'Paint',
  starterId: 7,
  startedAt: _base,
  viewerIds: viewers,
);

void _withApp(
  void Function(
    FakeAsync async,
    ProviderContainer container,
    MockRepository repo,
    String server,
  )
  body,
) {
  fakeAsync((async) {
    final repo = MockRepository(
      clock: () => _base.add(async.elapsed),
      random: Random(3),
      simulateLife: false,
    );
    final container = ProviderContainer(
      overrides: [
        repositoryProvider.overrideWithValue(repo),
        keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
        clockProvider.overrideWithValue(() => _base.add(async.elapsed)),
      ],
    );
    container.read(eventPumpProvider);
    repo.start();
    async.elapse(const Duration(seconds: 2));
    body(async, container, repo, repo.servers.first.key);
    container.dispose();
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

void main() {
  test('sessions start, change and end; a deleted channel takes its own', () {
    _withApp((async, container, repo, server) {
      Map<int, ActivitySession> sessions() =>
          container.read(activitySessionsProvider(server));
      expect(sessions(), isEmpty);

      repo.emit(ActivitySessionStarted(server, _session(1)));
      repo.emit(ActivitySessionStarted(server, _session(2, channelId: 21)));
      repo.emit(ActivitySessionStarted(server, _session(3)));
      async.flushMicrotasks();
      expect(sessions().keys, [1, 2, 3]);

      repo.emit(ActivitySessionChanged(server, _session(1, viewers: [5, 9])));
      async.flushMicrotasks();
      expect(sessions()[1]!.viewerIds, [5, 9]);

      repo.emit(ActivitySessionEnded(server, 1, 20, ActivityEndReason.ended));
      repo.emit(ActivitySessionEnded(server, 3, 20, ActivityEndReason.plugin));
      async.flushMicrotasks();
      expect(sessions().keys, [2]);

      repo.emit(ChannelDeleted(server, 21));
      async.flushMicrotasks();
      expect(sessions(), isEmpty);
    });
  });

  test("another server's events leave this one alone", () {
    _withApp((async, container, repo, server) {
      repo.emit(ActivitySessionStarted(server, _session(1)));
      repo.emit(ActivitySessionStarted(_otherServer, _session(44)));
      repo.emit(
        const ActivitySessionEnded(
          _otherServer,
          1,
          20,
          ActivityEndReason.ended,
        ),
      );
      async.flushMicrotasks();

      expect(container.read(activitySessionsProvider(server)).keys, [1]);
      expect(container.read(activitySessionsProvider(_otherServer)).keys, [
        44,
      ]);
    });
  });

  // A new session's Ready holds every session the server runs; one the map
  // held from before (it ended while disconnected) goes.
  test('a Ready after events replaces the map', () {
    _withApp((async, container, repo, server) {
      repo.emit(ActivitySessionStarted(server, _session(1)));
      async.flushMicrotasks();
      expect(container.read(activitySessionsProvider(server)).keys, [1]);

      final mock = repo.world.servers.firstWhere((s) => s.key == server);
      final voiceChannel = mock.voice.keys.first;
      mock.activitySessions[8] = _session(8, channelId: voiceChannel);
      repo.debugDisconnect(server);
      async.flushMicrotasks();
      repo.debugReconnect(server);
      async.elapse(const Duration(seconds: 1));

      expect(container.read(activitySessionsProvider(server)).keys, [8]);
      expect(
        container.read(serverProvider(server)).data!.pluginActivities,
        mock.pluginActivities,
      );
      expect(mock.pluginActivities, isNotEmpty);
    });
  });

  test('registered activities follow their update', () {
    _withApp((async, container, repo, server) {
      const quiz = PluginActivity(
        pluginId: 'quiz',
        activityKey: 'round',
        name: 'Quiz',
        description: '',
        pluginName: 'Quiz',
      );
      repo.emit(PluginActivitiesUpdated(server, const [quiz]));
      async.flushMicrotasks();

      expect(container.read(serverProvider(server)).data!.pluginActivities, [
        quiz,
      ]);
    });
  });
}
