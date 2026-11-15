import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/model/permissions.dart';

export 'package:opencord/core/model/channel_kind.dart';

enum OverwriteTargetKind { role, member }

/// Whether a plugin channel can be opened now (plugins design §14.4).
enum PluginChannelState {
  available,

  /// Its plugin isn't running, or isn't installed.
  pluginStopped,

  /// Its plugin runs but no longer registers this type.
  typeGone,
}

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
    this.bitrate = defaultBitrate,
    this.userLimit = 0,
    this.textInVoice = true,
    this.pluginId,
    this.typeKey,
    this.pluginState,
  });

  /// A voice channel's bitrate in bits per second (Phase 2 plan §5.3).
  static const defaultBitrate = 64000;
  static const minBitrate = 8000;
  static const maxUserLimit = 99;

  final int id;
  final ChannelKind kind;
  final String name;
  final String? topic;

  /// The category this channel sits in.
  final int? parentId;
  final int position;
  final List<PermissionOverwrite> overwrites;

  /// Voice channels only: bits per second, before the server's cap.
  final int bitrate;

  /// Voice channels only: at most this many people; 0 means no limit.
  final int userLimit;

  /// Voice channels only: whether it holds messages too.
  final bool textInVoice;

  /// Plugin channels only: the plugin and the type it registered.
  final String? pluginId;
  final String? typeKey;
  final PluginChannelState? pluginState;

  bool get isCategory => kind == ChannelKind.category;
  bool get isAvailablePlugin =>
      kind == ChannelKind.plugin && pluginState == PluginChannelState.available;
  bool get isPlugin => kind == ChannelKind.plugin;

  Channel copyWith({
    ChannelKind? kind,
    String? name,
    String? Function()? topic,
    int? Function()? parentId,
    int? position,
    List<PermissionOverwrite>? overwrites,
    int? bitrate,
    int? userLimit,
    bool? textInVoice,
    String? Function()? pluginId,
    String? Function()? typeKey,
    PluginChannelState? Function()? pluginState,
  }) {
    return Channel(
      id: id,
      kind: kind ?? this.kind,
      name: name ?? this.name,
      topic: topic == null ? this.topic : topic(),
      parentId: parentId == null ? this.parentId : parentId(),
      position: position ?? this.position,
      overwrites: overwrites ?? this.overwrites,
      bitrate: bitrate ?? this.bitrate,
      userLimit: userLimit ?? this.userLimit,
      textInVoice: textInVoice ?? this.textInVoice,
      pluginId: pluginId == null ? this.pluginId : pluginId(),
      typeKey: typeKey == null ? this.typeKey : typeKey(),
      pluginState: pluginState == null ? this.pluginState : pluginState(),
    );
  }
}
