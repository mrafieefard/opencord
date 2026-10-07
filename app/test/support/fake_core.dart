import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/rust/core_api.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

/// The Rust core's calls, recorded and answered from fields the test sets.
class FakeCoreApi extends Fake implements CoreApi {
  final events = StreamController<core.CoreEvent>.broadcast();
  final calls = <String>[];
  var servers = <core.Server>[];
  var identityInfo = const core.IdentityInfo(
    publicKeyHex: 'abcd',
    fingerprint: 'ABCD-EFGH-IJKL-MNOP',
  );

  /// Per channel, newest first.
  final history = <int, List<core.Message>>{};

  /// Holds fetchMessages until completed, when set.
  Completer<void>? fetchGate;

  /// Thrown by fetchMessages, when set: something the core never says.
  Exception? fetchFailure;
  core.CoreError? sendError;
  core.AddServerOutcome? addOutcome;

  void emit(String server, core.CoreEventPayload payload) =>
      events.add(core.CoreEvent(serverKey: server, payload: payload));

  @override
  Stream<core.CoreEvent> eventStream() => events.stream;

  @override
  core.GeneratedIdentity identityGenerate() => core.GeneratedIdentity(
    secret: Uint8List.fromList([1, 2, 3]),
    info: identityInfo,
  );

  @override
  core.IdentityInfo identityCheck(List<int> secret, String displayName) {
    if (displayName.isEmpty) {
      throw const core.CoreError.invalidInput(message: 'Choose a name.');
    }
    return identityInfo;
  }

  @override
  core.IdentityInfo identityLoad(List<int> secret, String displayName) {
    calls.add('identityLoad:$displayName');
    return identityCheck(secret, displayName);
  }

  @override
  String identityBackupEncode(List<int> secret) => 'backup:${secret.join(',')}';

  @override
  Uint8List identityBackupDecode(String backup) {
    if (!backup.startsWith('backup:')) {
      throw const core.CoreError.invalidInput(message: 'Not a backup.');
    }
    return Uint8List.fromList([
      for (final part in backup.substring(7).split(',')) int.parse(part),
    ]);
  }

  @override
  List<core.Server> serversList() => servers;

  @override
  Future<core.AddServerOutcome> serverAdd(
    String linkOrAddress,
    String? claimToken,
  ) async {
    calls.add('add:$linkOrAddress');
    final outcome = addOutcome!;
    if (outcome case core.AddServerOutcome_Added(:final field0)) {
      servers = [...servers, field0];
    }
    return outcome;
  }

  @override
  Future<void> serverRetryNow(String serverKey) async =>
      calls.add('retry:$serverKey');

  @override
  Future<List<core.Message>> fetchMessages(
    String serverKey,
    int channelId,
    int? before,
    int limit,
  ) async {
    calls.add('fetch:$channelId');
    await fetchGate?.future;
    if (fetchFailure case final failure?) throw failure;
    return (history[channelId] ?? const []).take(limit).toList();
  }

  @override
  Future<core.Message> sendMessage(
    String serverKey,
    int channelId,
    String content,
    String nonce,
  ) async {
    if (sendError case final error?) throw error;
    return core.Message(
      id: 900,
      channelId: channelId,
      authorId: 1,
      content: content,
      createdAtMs: 0,
      nonce: nonce,
    );
  }

  @override
  Future<core.Channel> createChannel(
    String serverKey,
    core.ChannelKind kind,
    String name,
    String? topic,
    int? parentId,
  ) async {
    calls.add('createChannel:$name');
    return core.Channel(
      id: 77,
      kind: kind,
      name: name,
      topic: topic,
      parentId: parentId,
      position: 0,
      overwrites: const [],
    );
  }

  @override
  Future<core.Channel> setChannelOverwrite(
    String serverKey,
    int channelId,
    core.PermissionOverwrite overwrite,
  ) async {
    calls.add(
      'overwrite:$channelId:${overwrite.targetKind.name}:${overwrite.targetId}'
      ':allow=${overwrite.allow}:deny=${overwrite.deny}',
    );
    return core.Channel(
      id: channelId,
      kind: core.ChannelKind.text,
      name: 'private',
      position: 0,
      overwrites: [overwrite],
    );
  }

  @override
  Future<void> updatePresence(
    String serverKey,
    core.PresenceStatus status,
  ) async => calls.add('presence:$serverKey:${status.name}');
}
