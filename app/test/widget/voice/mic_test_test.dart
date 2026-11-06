// The microphone test's state (spec §8.4): in a call it deafens and puts
// everything back; leaving, joining or undeafening ends it.

import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/voice/mic_test.dart';

final _base = DateTime(2026, 11, 6, 12);

void _withApp(
  void Function(
    FakeAsync async,
    ProviderContainer container,
    MockRepository repo,
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
    body(async, container, repo);
    container.dispose();
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

/// The first server and its voice channels.
(String, List<int>) _voice(ProviderContainer container, MockRepository repo) {
  final server = repo.servers.first.key;
  final channels = container
      .read(serverProvider(server))
      .data!
      .channels
      .values
      .where((channel) => channel.kind == ChannelKind.voice)
      .map((channel) => channel.id)
      .toList();
  return (server, channels);
}

/// Joins the first voice channel, or the [index]th.
void _join(
  FakeAsync async,
  ProviderContainer container,
  MockRepository repo, {
  int index = 0,
}) {
  final (server, channels) = _voice(container, repo);
  container.read(voiceSessionProvider.notifier).join(server, channels[index]);
  async.elapse(const Duration(seconds: 1));
}

/// Runs [call] and lets the repository answer.
void _run(FakeAsync async, Future<void> Function() call) {
  call();
  async.elapse(const Duration(milliseconds: 100));
}

MicTestNotifier _test(ProviderContainer container) =>
    container.read(micTestProvider.notifier);

void main() {
  test('in_a_call_it_deafens_and_restores', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      final before = container.read(voiceSessionProvider);

      _run(async, _test(container).start);
      final during = container.read(voiceSessionProvider);
      final testing = (container.read(micTestProvider), repo.micTest);
      _run(async, _test(container).stop);
      final after = container.read(voiceSessionProvider);

      expect((before.muted, before.deafened), (false, false));
      expect((during.muted, during.deafened), (true, true));
      expect(testing, (true, true));
      expect((after.muted, after.deafened), (false, false));
      expect((container.read(micTestProvider), repo.micTest), (false, false));
    });
  });

  test('in_a_call_it_restores_muted_as_it_was', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      _run(async, container.read(voiceSessionProvider.notifier).toggleMute);

      _run(async, _test(container).start);
      final during = container.read(voiceSessionProvider).deafened;
      _run(async, _test(container).stop);
      final after = container.read(voiceSessionProvider);

      expect(during, isTrue);
      expect((after.muted, after.deafened), (true, false));
    });
  });

  test('already_deafened_stays_deafened', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      _run(async, container.read(voiceSessionProvider.notifier).toggleDeafen);

      _run(async, _test(container).start);
      final testing = repo.micTest;
      _run(async, _test(container).stop);
      final after = container.read(voiceSessionProvider);

      expect(testing, isTrue);
      expect((after.muted, after.deafened), (true, true));
      expect(repo.micTest, isFalse);
    });
  });

  test('out_of_a_call_it_does_not_touch_voice', () {
    _withApp((async, container, repo) {
      final before = container.read(voiceSessionProvider);

      _run(async, _test(container).start);
      final during = container.read(voiceSessionProvider);
      final testing = repo.micTest;
      _run(async, _test(container).stop);

      expect(testing, isTrue);
      expect(identical(during, before), isTrue);
      expect(identical(container.read(voiceSessionProvider), before), isTrue);
      expect(repo.micTest, isFalse);
    });
  });

  test('undeafening_ends_it', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      final session = container.read(voiceSessionProvider.notifier);

      // By the deafen toggle (a hotkey, the tray, the dock).
      _run(async, _test(container).start);
      _run(async, session.toggleDeafen);
      final byDeafen = (
        container.read(micTestProvider),
        repo.micTest,
        container.read(voiceSessionProvider).deafened,
      );
      // By unmuting, which undeafens too.
      _run(async, _test(container).start);
      _run(async, session.toggleMute);
      final byMute = (
        container.read(micTestProvider),
        repo.micTest,
        container.read(voiceSessionProvider).deafened,
      );

      expect(byDeafen, (false, false, false));
      expect(byMute, (false, false, false));
    });
  });

  test('leaving_voice_ends_it', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      _run(async, _test(container).start);

      _run(async, container.read(voiceSessionProvider.notifier).leave);
      final after = container.read(voiceSessionProvider);

      expect((container.read(micTestProvider), repo.micTest), (false, false));
      expect(
        (after.connected, after.muted, after.deafened),
        (false, false, false),
      );
    });
  });

  test('joining_voice_ends_it', () {
    _withApp((async, container, repo) {
      // Out of a call.
      _run(async, _test(container).start);
      _join(async, container, repo);
      final fromOutside = (
        container.read(micTestProvider),
        repo.micTest,
        container.read(voiceSessionProvider).deafened,
      );
      // Moving to another channel.
      _run(async, _test(container).start);
      _join(async, container, repo, index: 1);
      final fromAnother = (
        container.read(micTestProvider),
        repo.micTest,
        container.read(voiceSessionProvider).deafened,
      );

      expect(fromOutside, (false, false, false));
      expect(fromAnother, (false, false, false));
    });
  });

  test('a_failure_puts_everything_back', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      repo.micTestError = const RepoException(
        RepoErrorKind.other,
        'no microphone',
      );

      Object? failure;
      _test(container).start().then(
        (_) {},
        onError: (Object error) {
          failure = error;
        },
      );
      async.elapse(const Duration(milliseconds: 100));
      final after = container.read(voiceSessionProvider);

      expect(failure, isA<RepoException>());
      expect((container.read(micTestProvider), repo.micTest), (false, false));
      expect((after.muted, after.deafened), (false, false));
    });
  });

  test('start_stop_start', () {
    _withApp((async, container, repo) {
      _join(async, container, repo);
      final micTest = _test(container);

      micTest.start();
      micTest.stop();
      micTest.start();
      async.elapse(const Duration(milliseconds: 100));
      final on = (
        container.read(micTestProvider),
        repo.micTest,
        container.read(voiceSessionProvider).deafened,
      );
      micTest.stop();
      micTest.start();
      micTest.stop();
      async.elapse(const Duration(milliseconds: 100));
      final off = (
        container.read(micTestProvider),
        repo.micTest,
        container.read(voiceSessionProvider).deafened,
      );

      expect(on, (true, true, true));
      expect(off, (false, false, false));
    });
  });
}
