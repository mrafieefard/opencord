import 'package:flutter/foundation.dart';

/// The author of a message, soundboard effect, queued track, or voice tile.
@immutable
class Author {
  const Author({
    required this.name,
    this.avatarHash,
    this.role,
    this.isPlugin = false,
  });

  final String name;
  final String? avatarHash;
  final String? role;
  final bool isPlugin;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is Author &&
          runtimeType == other.runtimeType &&
          name == other.name &&
          avatarHash == other.avatarHash &&
          role == other.role &&
          isPlugin == other.isPlugin;

  @override
  int get hashCode => Object.hash(name, avatarHash, role, isPlugin);

  @override
  String toString() =>
      'Author(name: $name, avatarHash: $avatarHash, role: $role, isPlugin: $isPlugin)';
}
