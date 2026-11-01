import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/repository/repository.dart';

final _base = DateTime(2026, 11, 1, 14, 30);

void withMock(
  void Function(FakeAsync async, MockRepository repo, List<RepoEvent> events)
  body,
) {
  fakeAsync((async) {
    final repo = MockRepository(
      clock: () => _base.add(async.elapsed),
      random: Random(7),
      simulateLife: false,
    );
    final events = <RepoEvent>[];
    final subscription = repo.events.listen(events.add);
    repo.start();
    async.elapse(const Duration(seconds: 2));
    body(async, repo, events);
    subscription.cancel();
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

Future<PrivateContact> _contact(MockRepository repo, String peer) async =>
    (await repo.privateContacts()).firstWhere((c) => c.peer == peer);

void main() {
  final kai = fakeFingerprint('key-1001');
  final mira = fakeFingerprint('key-1002');

  group('MockRepository chat keys (design 2026-11-01)', () {
    test('the mock shows a verified chat and one whose keys changed', () {
      withMock((async, repo, events) {
        PrivateContact? miraContact;
        PrivateContact? kaiContact;
        _contact(repo, mira).then((c) => miraContact = c);
        _contact(repo, kai).then((c) => kaiContact = c);
        async.flushMicrotasks();

        expect(miraContact!.verification, PrivateVerification.verified);
        expect(kaiContact!.newKeys, isTrue);
      });
    });

    test('a chat has 16 words, the same each time', () {
      withMock((async, repo, events) {
        List<String>? first;
        List<String>? again;
        repo.privateWords(kai).then((w) => first = w);
        repo.privateWords(kai).then((w) => again = w);
        async.flushMicrotasks();

        expect(first, hasLength(16));
        expect(again, first);
      });
    });

    test('They match verifies the chat', () {
      withMock((async, repo, events) {
        repo.verifyPrivate(kai, matched: true);
        async.flushMicrotasks();

        final changed = events.whereType<PrivateContactChanged>().last;
        expect(changed.contact.peer, kai);
        expect(changed.contact.verification, PrivateVerification.verified);
      });
    });

    test("They don't match marks a mismatch", () {
      withMock((async, repo, events) {
        repo.verifyPrivate(mira, matched: false);
        async.flushMicrotasks();

        final changed = events.whereType<PrivateContactChanged>().last;
        expect(changed.contact.verification, PrivateVerification.mismatch);
      });
    });

    test('using the new keys clears them and says the keys changed', () {
      withMock((async, repo, events) {
        repo.acceptNewPrivateKeys(kai);
        async.flushMicrotasks();

        final changed = events.whereType<PrivateContactChanged>().last;
        expect(changed.contact.newKeys, isFalse);
        expect(changed.contact.verification, PrivateVerification.unverified);
        final added = events.whereType<PrivateMessageAdded>().last;
        expect(added.message.peer, kai);
        expect(added.message.kind, PrivateMessageKind.keysChanged);
      });
    });

    test('approving the other device gives it the keys', () {
      withMock((async, repo, events) {
        ({List<OwnDevice> devices, bool hasChats})? before;
        repo.ownDevices().then((d) => before = d);
        async.flushMicrotasks();
        expect(before!.devices, hasLength(2));
        expect(before!.hasChats, isTrue);
        final other = before!.devices.firstWhere(
          (d) => d.state == OwnDeviceState.other,
        );

        List<String>? words;
        repo.pairingWords(other.deviceId).then((w) => words = w);
        repo.approveOwnDevice(other.deviceId);
        async.flushMicrotasks();

        expect(words, hasLength(16));
        final left = events.whereType<OwnDevicesChanged>().last.devices;
        expect(left.map((d) => d.state), [OwnDeviceState.offering]);
      });
    });

    test('taking the offered keys, or refusing them', () {
      withMock((async, repo, events) {
        ({List<OwnDevice> devices, bool hasChats})? before;
        repo.ownDevices().then((d) => before = d);
        async.flushMicrotasks();
        final offering = before!.devices.firstWhere(
          (d) => d.state == OwnDeviceState.offering,
        );

        repo.acceptKeysFrom(offering.deviceId, matched: false);
        async.flushMicrotasks();
        final refused = events.whereType<OwnDevicesChanged>().last.devices;
        expect(refused.every((d) => d.state == OwnDeviceState.other), isTrue);
      });
    });

    test('not mine hides the device', () {
      withMock((async, repo, events) {
        ({List<OwnDevice> devices, bool hasChats})? before;
        repo.ownDevices().then((d) => before = d);
        async.flushMicrotasks();

        repo.rejectOwnDevice(before!.devices.first.deviceId);
        async.flushMicrotasks();

        expect(
          events.whereType<OwnDevicesChanged>().last.devices,
          hasLength(1),
        );
      });
    });
  });
}
