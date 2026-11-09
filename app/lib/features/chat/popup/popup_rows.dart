import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/suggestions.dart';

/// Where commands come from: Opencord itself, or one plugin.
@immutable
class PopupSource {
  const PopupSource({
    required this.id,
    required this.name,
    this.avatarHash = '',
    this.userId,
  });

  /// "" for Opencord, `plugin:<id>` for a plugin.
  final String id;
  final String name;
  final String avatarHash;

  /// The plugin's user, for its icon; null for Opencord.
  final int? userId;

  bool get isOpencord => id.isEmpty;

  @override
  bool operator ==(Object other) =>
      other is PopupSource &&
      other.id == id &&
      other.name == name &&
      other.avatarHash == avatarHash &&
      other.userId == userId;

  @override
  int get hashCode => Object.hash(id, name, avatarHash, userId);
}

/// One source's commands in the popup.
@immutable
class PopupGroup {
  const PopupGroup({
    required this.source,
    required this.rows,
    required this.firstIndex,
  });

  final PopupSource source;
  final List<CommandSuggestion> rows;

  /// This group's first row, counted across every group.
  final int firstIndex;
}

const _opencord = PopupSource(id: '', name: 'Opencord');

PopupSource _sourceOf(String id, ServerData data) {
  if (id.isEmpty) return _opencord;
  final pluginId = id.substring(pluginSourcePrefix.length);
  for (final user in data.pluginUsers.values) {
    if (user.pluginId == pluginId) {
      return PopupSource(
        id: id,
        name: user.name,
        avatarHash: user.avatarHash,
        userId: user.userId,
      );
    }
  }
  return PopupSource(id: id, name: pluginId);
}

/// [rows] by source: Opencord first, then the plugins in the order the
/// server listed them, which is the order they were installed.
List<PopupGroup> groupCommands(List<CommandSuggestion> rows, ServerData data) {
  final bySource = <String, List<CommandSuggestion>>{'': []};
  for (final row in rows) {
    (bySource[row.spec.source] ??= []).add(row);
  }
  final groups = <PopupGroup>[];
  var firstIndex = 0;
  for (final MapEntry(key: id, value: commands) in bySource.entries) {
    if (commands.isEmpty) continue;
    groups.add(
      PopupGroup(
        source: _sourceOf(id, data),
        rows: List.unmodifiable(commands),
        firstIndex: firstIndex,
      ),
    );
    firstIndex += commands.length;
  }
  return groups;
}

/// Every row, in the order the arrow keys walk them.
List<CommandSuggestion> flatRows(List<PopupGroup> groups) => [
  for (final group in groups) ...group.rows,
];

/// The row [by] steps from [at], wrapping around [count] rows.
int stepRow(int at, int by, int count) => count == 0 ? 0 : (at + by) % count;

int _groupOfRow(List<PopupGroup> groups, int row) {
  for (var index = groups.length - 1; index > 0; index--) {
    if (row >= groups[index].firstIndex) return index;
  }
  return 0;
}

/// The first row of the group [by] groups from the one holding row [at].
int stepGroup(List<PopupGroup> groups, int at, int by) {
  if (groups.isEmpty) return 0;
  final next = (_groupOfRow(groups, at) + by) % groups.length;
  return groups[next].firstIndex;
}

double _heightOf(PopupGroup group, double row, double header) =>
    header + group.rows.length * row;

/// The scroll offset that puts row [index] just under its sticky header.
double offsetOfRow(
  List<PopupGroup> groups,
  int index, {
  required double row,
  required double header,
}) {
  var offset = 0.0;
  for (final group in groups) {
    final last = group.firstIndex + group.rows.length;
    if (index < last) return offset + (index - group.firstIndex) * row;
    offset += _heightOf(group, row, header);
  }
  return offset;
}

/// The group shown at the top when scrolled to [offset].
int groupAtOffset(
  List<PopupGroup> groups,
  double offset, {
  required double row,
  required double header,
}) {
  var top = 0.0;
  for (final (index, group) in groups.indexed) {
    top += _heightOf(group, row, header);
    if (offset < top) return index;
  }
  return groups.isEmpty ? 0 : groups.length - 1;
}
