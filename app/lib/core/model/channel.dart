import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/model/permissions.dart';

export 'package:opencord/core/model/channel_kind.dart';

enum OverwriteTargetKind { role, member }

@immutable
class PermissionOverwrite {
  const PermissionOverwrite({
    required this.targetKind,
    required this.targetId,
    required this.allow,
    required this.deny,
  });

  final OverwriteTargetKind targetKind;
  final int targetId;
  final Permissions allow;
  final Permissions deny;

  bool targets(OverwriteTargetKind kind, int id) =>
      targetKind == kind && targetId == id;

  @override
  bool operator ==(Object other) =>
      other is PermissionOverwrite &&
      other.targetKind == targetKind &&
      other.targetId == targetId &&
      other.allow == allow &&
      other.deny == deny;

  @override
  int get hashCode => Object.hash(targetKind, targetId, allow, deny);
}

/// A channel or a category (a category has kind [ChannelKind.category]).
@immutable
class Channel {
  const Channel({
    required this.id,
    required this.kind,
    required this.name,
    this.topic,
    this.parentId,
    this.position = 0,
    this.overwrites = const [],
  });

  final int id;
  final ChannelKind kind;
  final String name;
  final String? topic;

  /// The category this channel sits in.
  final int? parentId;
  final int position;
  final List<PermissionOverwrite> overwrites;

  bool get isCategory => kind == ChannelKind.category;

  Channel copyWith({
    ChannelKind? kind,
    String? name,
    String? Function()? topic,
    int? Function()? parentId,
    int? position,
    List<PermissionOverwrite>? overwrites,
  }) {
    return Channel(
      id: id,
      kind: kind ?? this.kind,
      name: name ?? this.name,
      topic: topic == null ? this.topic : topic(),
      parentId: parentId == null ? this.parentId : parentId(),
      position: position ?? this.position,
      overwrites: overwrites ?? this.overwrites,
    );
  }
}
