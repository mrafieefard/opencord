import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/permissions.dart';

/// Someone's identity on a server; the fingerprint is derived from their
/// public key.
@immutable
class User {
  const User({
    required this.id,
    required this.displayName,
    this.publicKeyHex = '',
    this.fingerprint = '',
  });

  final int id;
  final String displayName;
  final String publicKeyHex;

  /// Like `ABCD-EFGH-IJKL-MNOP`.
  final String fingerprint;

  User copyWith({String? displayName}) => User(
    id: id,
    displayName: displayName ?? this.displayName,
    publicKeyHex: publicKeyHex,
    fingerprint: fingerprint,
  );
}

@immutable
class Member {
  const Member({
    required this.user,
    required this.joinedAt,
    this.nickname,
    this.roleIds = const [],
  });

  final User user;
  final String? nickname;

  /// Without @everyone.
  final List<int> roleIds;
  final DateTime joinedAt;

  int get id => user.id;

  String get displayName => nickname ?? user.displayName;

  Member copyWith({
    User? user,
    String? Function()? nickname,
    List<int>? roleIds,
  }) => Member(
    user: user ?? this.user,
    nickname: nickname == null ? this.nickname : nickname(),
    roleIds: roleIds ?? this.roleIds,
    joinedAt: joinedAt,
  );
}

@immutable
class Role {
  const Role({
    required this.id,
    required this.name,
    required this.position,
    required this.permissions,
    this.color = 0,
    this.hoist = false,
    this.mentionable = false,
  });

  final int id;
  final String name;

  /// Kept for the protocol; the UI never shows role colors (§4.5).
  final int color;

  /// 0 is @everyone; higher ranks above.
  final int position;
  final Permissions permissions;

  /// Shown as its own group in the member list.
  final bool hoist;
  final bool mentionable;

  Role copyWith({
    String? name,
    int? position,
    Permissions? permissions,
    bool? hoist,
    bool? mentionable,
  }) => Role(
    id: id,
    name: name ?? this.name,
    color: color,
    position: position ?? this.position,
    permissions: permissions ?? this.permissions,
    hoist: hoist ?? this.hoist,
    mentionable: mentionable ?? this.mentionable,
  );
}
