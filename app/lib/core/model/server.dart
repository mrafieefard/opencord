import 'package:flutter/foundation.dart';

/// A server in the local list. [key] (`host:port`) identifies it in every
/// other call.
@immutable
class ServerSummary {
  const ServerSummary({
    required this.key,
    required this.name,
    this.fingerprint,
    this.userId,
  });

  final String key;
  final String name;

  /// Pinned certificate fingerprint (lowercase hex SHA-256), if any.
  final String? fingerprint;
  final int? userId;

  String get host => key.substring(0, key.lastIndexOf(':'));

  int get port => int.parse(key.substring(key.lastIndexOf(':') + 1));
}

@immutable
class ServerInfo {
  const ServerInfo({
    required this.name,
    required this.everyoneRoleId,
    this.serverIdHex = '',
    this.description = '',
    this.ownerId,
    this.openJoin = false,
  });

  final String serverIdHex;
  final String name;
  final String description;
  final int? ownerId;
  final bool openJoin;
  final int everyoneRoleId;

  ServerInfo copyWith({String? name, String? description, bool? openJoin}) =>
      ServerInfo(
        serverIdHex: serverIdHex,
        name: name ?? this.name,
        description: description ?? this.description,
        ownerId: ownerId,
        openJoin: openJoin ?? this.openJoin,
        everyoneRoleId: everyoneRoleId,
      );
}

enum ConnectionPhase { connecting, connected, reconnecting, failed }

enum FailureReason {
  /// Not a member, a bad invite or claim token, or banned before connecting.
  rejected,
  kicked,
  banned,

  /// The certificate no longer matches the pinned fingerprint.
  fingerprintChanged,

  /// The server speaks another protocol version.
  incompatible,
}

@immutable
class ConnectionStatus {
  const ConnectionStatus._(
    this.phase, {
    this.attempt = 0,
    this.retryAt,
    this.failure,
    this.message,
  });

  const ConnectionStatus.connecting() : this._(ConnectionPhase.connecting);

  const ConnectionStatus.connected() : this._(ConnectionPhase.connected);

  const ConnectionStatus.reconnecting({
    required int attempt,
    required DateTime retryAt,
  }) : this._(ConnectionPhase.reconnecting, attempt: attempt, retryAt: retryAt);

  const ConnectionStatus.failed(FailureReason reason, String message)
    : this._(ConnectionPhase.failed, failure: reason, message: message);

  final ConnectionPhase phase;
  final int attempt;

  /// When the next attempt starts, while reconnecting.
  final DateTime? retryAt;
  final FailureReason? failure;
  final String? message;

  bool get isConnected => phase == ConnectionPhase.connected;

  @override
  bool operator ==(Object other) =>
      other is ConnectionStatus &&
      other.phase == phase &&
      other.attempt == attempt &&
      other.retryAt == retryAt &&
      other.failure == failure &&
      other.message == message;

  @override
  int get hashCode => Object.hash(phase, attempt, retryAt, failure, message);
}
