/// Chat commands (Phase 3 plan §17), as the server describes them, and
/// the app's own two (`/volume`, `/music`), which never reach the server.
library;

import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/permissions.dart';

enum CommandOptionKind { text, integer, boolean, choice, user, channel }

/// Where a command's reply stands.
enum CommandRunState { answered, thinking, noAnswer }

/// A plugin command's [CommandSpec.feature] starts with this.
const pluginSourcePrefix = 'plugin:';

@immutable
class CommandChoice {
  const CommandChoice({
    required this.name,
    required this.value,
    this.artworkId = '',
  });

  /// What the user sees.
  final String name;

  /// What is sent.
  final String value;

  /// A track's artwork, for /play's suggestions.
  final String artworkId;

  @override
  bool operator ==(Object other) =>
      other is CommandChoice &&
      other.name == name &&
      other.value == value &&
      other.artworkId == artworkId;

  @override
  int get hashCode => Object.hash(name, value, artworkId);
}

@immutable
class CommandOptionSpec {
  const CommandOptionSpec({
    required this.name,
    required this.description,
    required this.kind,
    this.required = false,
    this.autocomplete = false,
    this.choices = const [],
    this.min = 0,
    this.max = 0,
  });

  final String name;
  final String description;
  final CommandOptionKind kind;
  final bool required;
  final bool autocomplete;
  final List<CommandChoice> choices;

  /// For integers; both 0 for no bounds.
  final int min;
  final int max;

  @override
  bool operator ==(Object other) =>
      other is CommandOptionSpec &&
      other.name == name &&
      other.description == description &&
      other.kind == kind &&
      other.required == required &&
      other.autocomplete == autocomplete &&
      listEquals(other.choices, choices) &&
      other.min == min &&
      other.max == max;

  @override
  int get hashCode => Object.hash(
    name,
    description,
    kind,
    required,
    autocomplete,
    Object.hashAll(choices),
    min,
    max,
  );
}

@immutable
class CommandSpec {
  const CommandSpec({
    required this.name,
    required this.description,
    this.options = const [],
    this.feature = '',
    this.permissionSummary = '',
    this.aliases = const [],
    this.permissions = Permissions.none,
    this.enabled = true,
    this.channelIds = const [],
    this.local = false,
  });

  final String name;
  final String description;
  final List<CommandOptionSpec> options;

  /// "music".
  final String feature;

  /// "Control playback".
  final String permissionSummary;
  final List<String> aliases;

  /// The least it needs; the server checks more.
  final Permissions permissions;
  final bool enabled;

  /// Empty: every channel.
  final List<int> channelIds;

  /// The app's own: handled here, never sent.
  final bool local;

  bool matches(String name) => this.name == name || aliases.contains(name);

  /// Whose command it is: empty for Opencord's, `plugin:<id>` for a plugin's.
  String get source => feature.startsWith(pluginSourcePrefix) ? feature : '';

  /// The plugin's id; null for Opencord's.
  String? get pluginId =>
      source.isEmpty ? null : source.substring(pluginSourcePrefix.length);

  bool allowedIn(int channelId) =>
      channelIds.isEmpty || channelIds.contains(channelId);

  CommandSpec copyWith({bool? enabled, List<int>? channelIds}) => CommandSpec(
    name: name,
    description: description,
    options: options,
    feature: feature,
    permissionSummary: permissionSummary,
    aliases: aliases,
    permissions: permissions,
    enabled: enabled ?? this.enabled,
    channelIds: channelIds ?? this.channelIds,
    local: local,
  );

  @override
  bool operator ==(Object other) =>
      other is CommandSpec &&
      other.name == name &&
      other.description == description &&
      listEquals(other.options, options) &&
      other.feature == feature &&
      other.permissionSummary == permissionSummary &&
      listEquals(other.aliases, aliases) &&
      other.permissions == permissions &&
      other.enabled == enabled &&
      listEquals(other.channelIds, channelIds) &&
      other.local == local;

  @override
  int get hashCode => Object.hash(
    name,
    description,
    Object.hashAll(options),
    feature,
    permissionSummary,
    Object.hashAll(aliases),
    permissions,
    enabled,
    Object.hashAll(channelIds),
    local,
  );
}

@immutable
class CommandList {
  const CommandList({this.commands = const [], this.resultsPublic = true});

  final List<CommandSpec> commands;

  /// Results go to the channel, or only to who ran the command.
  final bool resultsPublic;

  /// With a [source], only that source's. Without one: Opencord's, then the
  /// only plugin with the name; two plugins are never guessed between.
  CommandSpec? find(String name, {String source = ''}) {
    final named = commands.where((spec) => spec.matches(name));
    if (source.isNotEmpty) {
      return named.where((spec) => spec.source == source).firstOrNull;
    }
    final opencord = named.where((spec) => spec.source.isEmpty).firstOrNull;
    if (opencord != null) return opencord;
    return named.length == 1 ? named.first : null;
  }

  @override
  bool operator ==(Object other) =>
      other is CommandList &&
      listEquals(other.commands, commands) &&
      other.resultsPublic == resultsPublic;

  @override
  int get hashCode => Object.hash(Object.hashAll(commands), resultsPublic);
}

/// What running a command did.
typedef CommandOutcome = ({int? messageId, bool ephemeral});

/// A reply only this device sees, never stored (§17.3): from the server,
/// or an error the app shows the same way.
@immutable
class EphemeralNote {
  const EphemeralNote({
    required this.channelId,
    required this.nonce,
    required this.content,
    required this.at,
    this.error = false,
    this.state = CommandRunState.answered,
  });

  final int channelId;
  final String nonce;
  final String content;
  final DateTime at;
  final bool error;
  final CommandRunState state;
}

/// `/volume` and `/music` (§17.2): the app's own.
const localCommands = [
  CommandSpec(
    name: 'volume',
    description: 'Your music volume, on every server',
    options: [
      CommandOptionSpec(
        name: 'percent',
        description: '0 to 200',
        kind: CommandOptionKind.integer,
        required: true,
        max: 200,
      ),
    ],
    feature: 'music',
    permissionSummary: 'Anyone',
    local: true,
  ),
  CommandSpec(
    name: 'music',
    description: 'Open the music panel',
    feature: 'music',
    permissionSummary: 'Anyone',
    local: true,
  ),
];
