import 'dart:convert';
import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/repository/repository.dart';

// The mock's activities answer like the server and the core (plugins M3
// Task 9): one session per activity and call, at most four, the viewer list
// following who has it open, and views closed with the real reasons.

final _base = DateTime(2026, 11, 12, 14);
const _server = 'opencord.example:7710';

class _World {
  _World(this.async, this.repo, this.events, this.outputs);

  final FakeAsync async;
  final MockRepository repo;
  final List<RepoEvent> events;
  final List<ViewOutput> outputs;

  MockServer get server =>
      repo.world.servers.firstWhere((server) => server.key == _server);

  int get general => server.channels.values
      .firstWhere((c) => c.kind == ChannelKind.voice && c.name == 'General')
      .id;

  int get pairing => server.channels.values
      .firstWhere((c) => c.kind == ChannelKind.voice && c.name == 'Pairing')
      .id;

  /// Runs [future] to its end on fake time; its error is thrown here.
  T run<T>(Future<T> future) {
    T? value;
    Object? error;
    var done = false;
    future.then(
      (result) {
        value = result;
        done = true;
      },
      onError: (Object failure) {
        error = failure;
        done = true;
      },
    );
    async.flushMicrotasks();
    expect(done, isTrue, reason: 'the call never finished');
    if (error case final failure?) throw failure;
    return value as T;
  }

  ActivitySession start([String pluginId = 'paint', String key = 'paint']) =>
      run(repo.startActivity(_server, general, pluginId, key));

  /// The `closed` reason of [viewId]'s last batch, or null.
  String? closedReason(String viewId) {
    final batches = outputs.where((output) => output.viewId == viewId);
    if (batches.isEmpty) return null;
    final ops = (jsonDecode(batches.last.json) as Map)['ops'] as List;
    for (final op in ops.cast<Map>()) {
      if (op['op'] == 'closed') return op['reason'] as String;
    }
    return null;
  }
}

void _withMock(void Function(_World world) body) {
  fakeAsync((async) {
    final repo = MockRepository(
      clock: () => _base.add(async.elapsed),
      random: Random(7),
      simulateLife: false,
    );
    final events = <RepoEvent>[];
    final outputs = <ViewOutput>[];
    final subscriptions = [
      repo.events.listen(events.add),
      repo.viewOutputs.listen(outputs.add),
    ];
    repo.start();
    async.elapse(const Duration(seconds: 2));
    body(_World(async, repo, events, outputs));
    for (final subscription in subscriptions) {
      subscription.cancel();
    }
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

Matcher _refused(RepoErrorKind kind) =>
    throwsA(isA<RepoException>().having((error) => error.kind, 'kind', kind));

void main() {
  test('the Ready lists the registered activity', () {
    _withMock((world) {
      final ready = world.events.whereType<Ready>().firstWhere(
        (event) => event.serverKey == _server,
      );

      expect(ready.snapshot.pluginActivities, const [
        PluginActivity(
          pluginId: 'paint',
          activityKey: 'paint',
          name: 'Paint together',
          description: 'A shared whiteboard',
          pluginName: 'Paint',
        ),
      ]);
      expect(world.repo.capabilities.activities, isTrue);
    });
  });

  test('starting twice gives the running session, announced once', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.general));

      final first = world.start();
      final second = world.start();

      expect(second, first);
      expect(
        (first.channelId, first.pluginId, first.activityKey, first.starterId),
        (world.general, 'paint', 'paint', world.server.selfId),
      );
      expect(first.viewerIds, isEmpty);
      expect(world.events.whereType<ActivitySessionStarted>(), hasLength(1));
      expect(
        world.events.whereType<ActivitySessionStarted>().single.session,
        first,
      );
    });
  });

  test('starting outside the call, unregistered or a fifth is refused', () {
    _withMock((world) {
      expect(world.start, _refused(RepoErrorKind.invalidArgument));

      world.run(world.repo.joinVoice(_server, world.general));
      expect(() => world.start('paint', 'nope'), _refused(RepoErrorKind.other));

      for (final key in ['a', 'b', 'c', 'd', 'e']) {
        world.server.pluginActivities.add(
          PluginActivity(
            pluginId: 'more',
            activityKey: key,
            name: key,
            description: '',
            pluginName: 'More',
          ),
        );
      }
      for (final key in ['a', 'b', 'c', 'd']) {
        world.start('more', key);
      }
      expect(
        () => world.start('more', 'e'),
        throwsA(
          isA<RepoException>().having(
            (error) => error.message,
            'message',
            'this call already has 4 activities',
          ),
        ),
      );
      // A running one still comes back at the cap.
      expect(world.start('more', 'a').activityKey, 'a');
    });
  });

  test('opening adds the viewer, its first batch waits for the caller, and '
      'closing removes the viewer', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.general));
      final session = world.start();

      // The caller holds the view's id before its first batch arrives, so
      // a listener that filters by id doesn't miss it.
      String? openedId;
      var batchAfterId = false;
      final listening = world.repo.viewOutputs.listen((output) {
        if (output.viewId == openedId) batchAfterId = true;
      });
      world.repo
          .openActivityView(_server, session, '{}')
          .then((id) => openedId = id);
      world.async.flushMicrotasks();
      listening.cancel();
      expect(batchAfterId, isTrue);
      final viewId = openedId!;

      final first = world.outputs.where((o) => o.viewId == viewId).single;
      expect(first.json, contains('Count: 0'));
      final changed = world.events.whereType<ActivitySessionChanged>().last;
      expect(changed.session.id, session.id);
      expect(changed.session.viewerIds, [world.server.selfId]);

      world.repo.viewInput(
        viewId,
        '{"kind":"event","node":3,"event":"press","data":{}}',
      );
      world.async.flushMicrotasks();
      expect(world.outputs.last.json, contains('Count: 1'));

      world.run(world.repo.closeView(viewId));
      expect(
        world.events.whereType<ActivitySessionChanged>().last.session.viewerIds,
        isEmpty,
      );
      expect(world.server.activitySessions[session.id]!.viewerIds, isEmpty);
    });
  });

  test('opening without being in the call, or a gone session, is refused', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.general));
      final session = world.start();
      world.run(world.repo.joinVoice(_server, world.pairing));

      expect(
        () => world.run(world.repo.openActivityView(_server, session, '{}')),
        _refused(RepoErrorKind.forbidden),
      );

      world.run(world.repo.joinVoice(_server, world.general));
      world.run(world.repo.endActivity(_server, session.id));
      expect(
        () => world.run(world.repo.openActivityView(_server, session, '{}')),
        _refused(RepoErrorKind.notFound),
      );
    });
  });

  test('ending closes its views with activity_ended and tells everyone', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.general));
      final session = world.start();
      final viewId = world.run(
        world.repo.openActivityView(_server, session, '{}'),
      );

      world.run(world.repo.endActivity(_server, session.id));

      expect(world.closedReason(viewId), 'activity_ended');
      final ended = world.events.whereType<ActivitySessionEnded>().single;
      expect(
        (ended.sessionId, ended.channelId, ended.reason),
        (session.id, world.general, ActivityEndReason.ended),
      );
      expect(world.server.activitySessions, isEmpty);
      // The closed view hears nothing more.
      final batches = world.outputs.length;
      world.repo.viewInput(
        viewId,
        '{"kind":"event","node":3,"event":"press","data":{}}',
      );
      world.async.flushMicrotasks();
      expect(world.outputs, hasLength(batches));
    });
  });

  test('only its starter or a channel manager ends it', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.general));
      final self = world.server.self;
      const kai = 1001;
      world.server.activitySessions[77] = ActivitySession(
        id: 77,
        channelId: world.general,
        pluginId: 'paint',
        activityKey: 'paint',
        name: 'Paint together',
        icon: '',
        pluginName: 'Paint',
        starterId: kai,
        startedAt: _base,
        viewerIds: const [],
      );
      // The owner holds every permission, so self stops being the owner and
      // loses its roles: neither starter nor channel manager.
      final info = world.server.info;
      world.server.info = ServerInfo(
        serverIdHex: info.serverIdHex,
        name: info.name,
        description: info.description,
        ownerId: kai,
        openJoin: info.openJoin,
        everyoneRoleId: info.everyoneRoleId,
        iconHash: info.iconHash,
      );
      world.server.members[self.id] = self.copyWith(roleIds: const []);

      expect(
        () => world.run(world.repo.endActivity(_server, 77)),
        _refused(RepoErrorKind.forbidden),
      );
      expect(world.server.activitySessions.keys, [77]);

      world.server.members[self.id] = self;
      world.server.info = info;
      world.run(world.repo.endActivity(_server, 77));
      expect(world.server.activitySessions, isEmpty);
    });
  });

  test('leaving the call closes the view with left_call; the session goes '
      'on while others are in it', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.general));
      final session = world.start();
      final viewId = world.run(
        world.repo.openActivityView(_server, session, '{}'),
      );

      world.run(world.repo.leaveVoice());

      expect(world.closedReason(viewId), 'left_call');
      expect(
        world.events.whereType<ActivitySessionChanged>().last.session.viewerIds,
        isEmpty,
      );
      expect(world.server.activitySessions.keys, [session.id]);
      expect(world.events.whereType<ActivitySessionEnded>(), isEmpty);
    });
  });

  test('the call emptying ends its sessions', () {
    _withMock((world) {
      world.run(world.repo.joinVoice(_server, world.pairing));
      final session = world.run(
        world.repo.startActivity(_server, world.pairing, 'paint', 'paint'),
      );

      world.run(world.repo.leaveVoice());

      final ended = world.events.whereType<ActivitySessionEnded>().single;
      expect(
        (ended.sessionId, ended.reason),
        (session.id, ActivityEndReason.callEmpty),
      );
      expect(world.server.activitySessions, isEmpty);
    });
  });
}
