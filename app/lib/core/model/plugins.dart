import 'package:flutter/foundation.dart';

enum PluginState { starting, running, stopped, crashed }

enum PluginSignature { unsigned, signed, dev }

enum GithubCheck { unspecified, verified, mismatch, failed }

enum PluginConfigKind {
  text,
  secret,
  number,
  boolean,
  choice,
  role,
  channel,
  user,
}

enum PluginLogLevel { info, warn, error, action }

@immutable
class PluginConfigField {
  const PluginConfigField({
    required this.key,
    required this.label,
    required this.kind,
    this.choices = const [],
    this.required = false,
  });

  final String key;
  final String label;
  final PluginConfigKind kind;
  final List<String> choices;
  final bool required;
}

@immutable
class PluginConfigValue {
  const PluginConfigValue({
    required this.key,
    required this.value,
    this.hasValue = false,
  });

  final String key;
  final String value;
  final bool hasValue;
}

@immutable
class PluginPermissionGrant {
  const PluginPermissionGrant({
    required this.name,
    required this.granted,
    required this.grantable,
  });

  final String name;
  final bool granted;
  final bool grantable;
}

@immutable
class PluginUser {
  const PluginUser({
    required this.userId,
    required this.pluginId,
    required this.name,
    this.avatarHash = '',
    this.installed = true,
  });

  final int userId;
  final String pluginId;
  final String name;
  final String avatarHash;
  final bool installed;
}

@immutable
class Plugin {
  const Plugin({
    required this.id,
    required this.name,
    required this.version,
    required this.author,
    required this.description,
    this.github = '',
    this.license = '',
    this.signature = PluginSignature.unsigned,
    this.keyFingerprint = '',
    this.githubCheck = GithubCheck.unspecified,
    this.state = PluginState.starting,
    this.statusReason = '',
    this.permissions = const [],
    this.domains = const [],
    this.configFields = const [],
    this.config = const [],
    required this.userId,
    required this.roleId,
    required this.installedAt,
    required this.updatedAt,
    this.cpuPermille = 0,
    this.memoryBytes = 0,
    this.databaseBytes = 0,
    this.filesBytes = 0,
  });

  final String id;
  final String name;
  final String version;
  final String author;
  final String description;
  final String github;
  final String license;
  final PluginSignature signature;
  final String keyFingerprint;
  final GithubCheck githubCheck;
  final PluginState state;
  final String statusReason;
  final List<PluginPermissionGrant> permissions;
  final List<String> domains;
  final List<PluginConfigField> configFields;
  final List<PluginConfigValue> config;
  final int userId;
  final int roleId;
  final DateTime installedAt;
  final DateTime updatedAt;
  final int cpuPermille;
  final int memoryBytes;
  final int databaseBytes;
  final int filesBytes;

  Plugin copyWith({
    String? name,
    String? version,
    String? author,
    String? description,
    String? github,
    String? license,
    PluginSignature? signature,
    String? keyFingerprint,
    GithubCheck? githubCheck,
    PluginState? state,
    String? statusReason,
    List<PluginPermissionGrant>? permissions,
    List<String>? domains,
    List<PluginConfigField>? configFields,
    List<PluginConfigValue>? config,
    int? cpuPermille,
    int? memoryBytes,
    int? databaseBytes,
    int? filesBytes,
  }) => Plugin(
    id: id,
    name: name ?? this.name,
    version: version ?? this.version,
    author: author ?? this.author,
    description: description ?? this.description,
    github: github ?? this.github,
    license: license ?? this.license,
    signature: signature ?? this.signature,
    keyFingerprint: keyFingerprint ?? this.keyFingerprint,
    githubCheck: githubCheck ?? this.githubCheck,
    state: state ?? this.state,
    statusReason: statusReason ?? this.statusReason,
    permissions: permissions ?? this.permissions,
    domains: domains ?? this.domains,
    configFields: configFields ?? this.configFields,
    config: config ?? this.config,
    userId: userId,
    roleId: roleId,
    installedAt: installedAt,
    updatedAt: updatedAt,
    cpuPermille: cpuPermille ?? this.cpuPermille,
    memoryBytes: memoryBytes ?? this.memoryBytes,
    databaseBytes: databaseBytes ?? this.databaseBytes,
    filesBytes: filesBytes ?? this.filesBytes,
  );
}

@immutable
class PluginReview {
  const PluginReview({
    required this.reviewId,
    required this.plugin,
    this.installed,
    this.newPermissions = const [],
    this.licenseNeedsAcceptance = false,
    required this.expiresAt,
    this.dev = false,
  });

  final String reviewId;
  final Plugin plugin;
  final Plugin? installed;
  final List<String> newPermissions;
  final bool licenseNeedsAcceptance;
  final DateTime expiresAt;
  final bool dev;
}

@immutable
class PluginLogLine {
  const PluginLogLine({
    required this.seq,
    required this.at,
    required this.level,
    required this.text,
  });

  final int seq;
  final DateTime at;
  final PluginLogLevel level;
  final String text;
}

@immutable
class PluginDevToken {
  const PluginDevToken({
    required this.id,
    required this.pluginId,
    required this.createdBy,
    required this.createdAt,
    required this.expiresAt,
    this.lastUsedAt,
  });

  final int id;
  final String pluginId;
  final int createdBy;
  final DateTime createdAt;
  final DateTime expiresAt;
  final DateTime? lastUsedAt;
}

@immutable
class CreatedPluginDevToken {
  const CreatedPluginDevToken({
    required this.token,
    required this.secret,
    required this.serverFingerprint,
  });

  final PluginDevToken token;
  final String secret;
  final String serverFingerprint;
}

/// A channel type a running plugin registers (plugins design §14.1).
@immutable
class PluginChannelType {
  const PluginChannelType({
    required this.pluginId,
    required this.typeKey,
    required this.name,
    required this.description,
    this.icon = '',
    required this.pluginName,
  });

  final String pluginId;
  final String typeKey;
  final String name;
  final String description;
  final String icon;
  final String pluginName;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PluginChannelType &&
          other.pluginId == pluginId &&
          other.typeKey == typeKey &&
          other.name == name &&
          other.description == description &&
          other.icon == icon &&
          other.pluginName == pluginName;

  @override
  int get hashCode =>
      Object.hash(pluginId, typeKey, name, description, icon, pluginName);
}

/// A batch for the renderer of one plugin view: the core's id for the view
/// and the batch as JSON (plugins milestone 2 plan, "The view wire format").
@immutable
class ViewOutput {
  const ViewOutput({required this.viewId, required this.json});

  final String viewId;
  final String json;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is ViewOutput && other.viewId == viewId && other.json == json;

  @override
  int get hashCode => Object.hash(viewId, json);
}
