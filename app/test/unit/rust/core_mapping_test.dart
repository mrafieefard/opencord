import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int64List;
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/core_mapping.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

const _user = core.User(
  id: 7,
  publicKeyHex: 'ab12',
  fingerprint: 'ABCD-EFGH-IJKL-MNOP',
  displayName: 'Kai',
);

void main() {
  test('messages keep ids, times and the nonce', () {
    final message = messageFrom(
      const core.Message(
        id: 10,
        channelId: 2,
        authorId: 7,
        content: 'hi',
        createdAtMs: 1760000000000,
        editedAtMs: 1760000005000,
        nonce: 'n1',
      ),
    );

    expect(message.id, 10);
    expect(message.channelId, 2);
    expect(
      message.createdAt,
      DateTime.fromMillisecondsSinceEpoch(1760000000000),
    );
    expect(
      message.editedAt,
      DateTime.fromMillisecondsSinceEpoch(1760000005000),
    );
    expect(message.nonce, 'n1');
  });

  test('channels keep their kind, place and overwrites', () {
    final channel = channelFrom(
      const core.Channel(
        id: 3,
        kind: core.ChannelKind.voice,
        name: 'Lounge',
        parentId: 1,
        position: 2,
        overwrites: [
          core.PermissionOverwrite(
            targetKind: core.OverwriteTargetKind.role,
            targetId: 1,
            allow: 0,
            deny: 1,
          ),
        ],
      ),
    );

    expect(channel.kind, ChannelKind.voice);
    expect(channel.parentId, 1);
    expect(channel.overwrites.single.deny, Permissions.viewChannel);
    expect(channel.overwrites.single.targetKind, OverwriteTargetKind.role);
  });

  test('the administrator bit survives as a negative i64', () {
    final role = roleFrom(
      const core.Role(
        id: 1,
        name: 'Admin',
        color: 0,
        position: 3,
        permissions: -9223372036854775808,
        hoist: true,
        mentionable: false,
      ),
    );

    expect(role.permissions.has(Permissions.administrator), isTrue);
  });

  test('members carry their user, nickname and roles', () {
    final member = memberFrom(
      core.Member(
        user: _user,
        nickname: 'K',
        roleIds: Int64List.fromList([4, 5]),
        joinedAtMs: 1760000000000,
      ),
    );

    expect(member.displayName, 'K');
    expect(member.user.fingerprint, 'ABCD-EFGH-IJKL-MNOP');
    expect(member.roleIds, [4, 5]);
  });

  test('presence maps both ways, with invisible sent as offline', () {
    expect(presenceFrom(core.PresenceStatus.dnd), Presence.doNotDisturb);
    expect(presenceFrom(core.PresenceStatus.offline), Presence.offline);
    expect(presenceTo(SelfPresence.invisible), core.PresenceStatus.offline);
    expect(presenceTo(SelfPresence.idle), core.PresenceStatus.idle);
  });

  group('connection states', () {
    final now = DateTime(2026, 10, 7, 12);

    test('reconnecting turns the wait into a time', () {
      final status = connectionFrom(
        const core.ConnectionState.reconnecting(attempt: 2, retryInMs: 8000),
        now: now,
      );

      expect(status.phase, ConnectionPhase.reconnecting);
      expect(status.attempt, 2);
      expect(status.retryAt, now.add(const Duration(seconds: 8)));
    });

    test('a changed certificate keeps both fingerprints', () {
      final status = connectionFrom(
        const core.ConnectionState.failed(
          reason: core.FailureReason.fingerprintChanged,
          message: 'changed',
          expectedFingerprint: 'aa',
          presentedFingerprint: 'bb',
        ),
        now: now,
      );

      expect(status.failure, FailureReason.fingerprintChanged);
      expect(status.expectedFingerprint, 'aa');
      expect(status.presentedFingerprint, 'bb');
    });
  });

  test('a Ready snapshot lists presences by user', () {
    final snapshot = snapshotFrom(
      core.ReadySnapshot(
        selfUser: _user,
        server: const core.ServerInfo(
          serverIdHex: 'ff',
          name: 'Home',
          description: '',
          ownerId: 7,
          openJoin: false,
          everyoneRoleId: 1,
        ),
        channels: const [],
        roles: const [],
        members: const [],
        presences: const [
          core.Presence(userId: 7, status: core.PresenceStatus.idle),
        ],
        serverPermissions: 3,
        channelPermissions: const [
          core.ChannelPermissions(channelId: 2, permissions: 1),
        ],
      ),
    );

    expect(snapshot.self.id, 7);
    expect(snapshot.info.name, 'Home');
    expect(snapshot.presences, {7: Presence.idle});
    expect(snapshot.channelPermissions, {2: Permissions.viewChannel});
    expect(snapshot.serverPermissions.bits, 3);
  });

  group('errors', () {
    test('server errors keep their kind and wait', () {
      final error = errorFrom(
        const core.CoreError.server(
          code: core.ErrorCode.rateLimited,
          message: 'slow down',
          retryAfterMs: 1500,
        ),
      );

      expect(error.kind, RepoErrorKind.rateLimited);
      expect(error.message, 'slow down');
      expect(error.retryAfter, const Duration(milliseconds: 1500));
    });

    test('local errors read like the app', () {
      expect(
        errorFrom(const core.CoreError.notConnected()).kind,
        RepoErrorKind.notConnected,
      );
      expect(
        errorFrom(
          const core.CoreError.fingerprintMismatch(
            expected: 'aa',
            presented: 'bb',
          ),
        ).kind,
        RepoErrorKind.fingerprintMismatch,
      );
    });
  });

  test('adding a server either adds it or asks for trust', () {
    final needs = addServerFrom(
      const core.AddServerOutcome.needsTrust(
        address: 'a.example:7710',
        fingerprint: 'cc',
      ),
    );

    expect(needs, isA<ServerNeedsTrust>());
    expect((needs as ServerNeedsTrust).fingerprint, 'cc');
  });
}
