/// Lavalink nodes (Phase 3 plan §12): where music plays, as members who
/// manage the server see them. Passwords never reach the app.
library;

import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/network.dart';

enum LavalinkPriority {
  preferred('Preferred'),
  normal('Normal'),
  backup('Backup');

  const LavalinkPriority(this.label);

  final String label;
}

enum LavalinkHealth {
  connecting('Connecting'),
  healthy('Healthy'),
  degraded('Degraded'),
  unavailable('Unavailable'),
  draining('Draining'),
  disabled('Disabled');

  const LavalinkHealth(this.label);

  final String label;

  /// Plays music now.
  bool get isUp => this == healthy || this == degraded || this == draining;
}

/// What an admin sets for a node.
@immutable
class LavalinkNodeSettings {
  const LavalinkNodeSettings({
    required this.name,
    required this.host,
    this.port = 2333,
    this.secure = false,
    this.priority = LavalinkPriority.normal,
    this.maxPlayers = 0,
    this.strictAddress = true,
    this.enabled = true,
  });

  final String name;
  final String host;
  final int port;

  /// https and wss.
  final bool secure;
  final LavalinkPriority priority;

  /// At most this many channels at once; 0 for no cap.
  final int maxPlayers;

  /// The bridge accepts only this node's addresses.
  final bool strictAddress;
  final bool enabled;

  /// `host:port`, with brackets around an IPv6 address.
  String get address => host.contains(':') ? '[$host]:$port' : '$host:$port';

  LavalinkNodeSettings copyWith({
    String? name,
    String? host,
    int? port,
    bool? secure,
    LavalinkPriority? priority,
    int? maxPlayers,
    bool? strictAddress,
    bool? enabled,
  }) => LavalinkNodeSettings(
    name: name ?? this.name,
    host: host ?? this.host,
    port: port ?? this.port,
    secure: secure ?? this.secure,
    priority: priority ?? this.priority,
    maxPlayers: maxPlayers ?? this.maxPlayers,
    strictAddress: strictAddress ?? this.strictAddress,
    enabled: enabled ?? this.enabled,
  );

  @override
  bool operator ==(Object other) =>
      other is LavalinkNodeSettings &&
      other.name == name &&
      other.host == host &&
      other.port == port &&
      other.secure == secure &&
      other.priority == priority &&
      other.maxPlayers == maxPlayers &&
      other.strictAddress == strictAddress &&
      other.enabled == enabled;

  @override
  int get hashCode => Object.hash(
    name,
    host,
    port,
    secure,
    priority,
    maxPlayers,
    strictAddress,
    enabled,
  );
}

@immutable
class LavalinkNode {
  const LavalinkNode({
    required this.id,
    required this.settings,
    this.draining = false,
    this.health = LavalinkHealth.connecting,
    this.reason = '',
    this.version = '',
    this.sources = const [],
    this.plugins = const [],
    this.channels = 0,
    this.players = 0,
    this.playingPlayers = 0,
    this.cpuPercent = 0,
    this.pingMs = 0,
    this.lastError = '',
    this.connectedSince,
    this.recovering = false,
  });

  final int id;
  final LavalinkNodeSettings settings;
  final bool draining;
  final LavalinkHealth health;

  /// Why it is degraded or unavailable, in plain words.
  final String reason;
  final String version;
  final List<String> sources;
  final List<String> plugins;

  /// Opencord's channels on it, and every player it has.
  final int channels;
  final int players;
  final int playingPlayers;
  final int cpuPercent;
  final int pingMs;

  /// The last thing that went wrong, kept after it recovers.
  final String lastError;
  final DateTime? connectedSince;

  /// Back after being unavailable: new channels wait a little.
  final bool recovering;

  bool get connected => connectedSince != null;

  /// The same node, playing for [channels] Opencord channels.
  LavalinkNode copyWithChannels(int channels) => LavalinkNode(
    id: id,
    settings: settings,
    draining: draining,
    health: health,
    reason: reason,
    version: version,
    sources: sources,
    plugins: plugins,
    channels: channels,
    players: players,
    playingPlayers: playingPlayers,
    cpuPercent: cpuPercent,
    pingMs: pingMs,
    lastError: lastError,
    connectedSince: connectedSince,
    recovering: recovering,
  );

  @override
  bool operator ==(Object other) =>
      other is LavalinkNode &&
      other.id == id &&
      other.settings == settings &&
      other.draining == draining &&
      other.health == health &&
      other.reason == reason &&
      other.version == version &&
      listEquals(other.sources, sources) &&
      listEquals(other.plugins, plugins) &&
      other.channels == channels &&
      other.players == players &&
      other.playingPlayers == playingPlayers &&
      other.cpuPercent == cpuPercent &&
      other.pingMs == pingMs &&
      other.lastError == lastError &&
      other.connectedSince == connectedSince &&
      other.recovering == recovering;

  @override
  int get hashCode => Object.hash(
    id,
    settings,
    draining,
    health,
    reason,
    version,
    Object.hashAll(sources),
    Object.hashAll(plugins),
    channels,
    players,
    playingPlayers,
    cpuPercent,
    pingMs,
    lastError,
    connectedSince,
    recovering,
  );
}

enum LavalinkEventKind { connected, disconnected, degraded, recovered, moved }

/// A line in the node events.
@immutable
class LavalinkNodeEvent {
  const LavalinkNodeEvent({
    required this.nodeId,
    required this.kind,
    required this.text,
    required this.at,
  });

  final int nodeId;
  final LavalinkEventKind kind;
  final String text;
  final DateTime at;

  @override
  bool operator ==(Object other) =>
      other is LavalinkNodeEvent &&
      other.nodeId == nodeId &&
      other.kind == kind &&
      other.text == text &&
      other.at == at;

  @override
  int get hashCode => Object.hash(nodeId, kind, text, at);
}

/// What Lavalink needs from this server: the checklist.
@immutable
class LavalinkBridge {
  const LavalinkBridge({
    this.enabled = false,
    this.url = '',
    this.trustedCertificate = false,
    this.audioAddress = '',
    this.audioPort = 0,
    this.audioListening = false,
    this.mapping = MappingState.notUsed,
  });

  /// False when music is off on the server.
  final bool enabled;

  /// wss://host:port, where Lavalink connects.
  final String url;
  final bool trustedCertificate;

  /// address:port, where Lavalink sends audio; empty while unknown.
  final String audioAddress;
  final int audioPort;
  final bool audioListening;
  final MappingState mapping;

  @override
  bool operator ==(Object other) =>
      other is LavalinkBridge &&
      other.enabled == enabled &&
      other.url == url &&
      other.trustedCertificate == trustedCertificate &&
      other.audioAddress == audioAddress &&
      other.audioPort == audioPort &&
      other.audioListening == audioListening &&
      other.mapping == mapping;

  @override
  int get hashCode => Object.hash(
    enabled,
    url,
    trustedCertificate,
    audioAddress,
    audioPort,
    audioListening,
    mapping,
  );
}

/// At most this many nodes and events.
const maxLavalinkNodes = 20;
const lavalinkEventsKept = 50;

@immutable
class LavalinkNodes {
  const LavalinkNodes({
    this.nodes = const [],
    this.bridge = const LavalinkBridge(),
    this.events = const [],
    this.channelsPlaying = 0,
  });

  final List<LavalinkNode> nodes;
  final LavalinkBridge bridge;

  /// Newest first.
  final List<LavalinkNodeEvent> events;
  final int channelsPlaying;

  LavalinkNode? node(int id) {
    for (final node in nodes) {
      if (node.id == id) return node;
    }
    return null;
  }

  /// "3 nodes · 2 healthy · 7 channels playing".
  String get summary {
    final healthy = nodes
        .where((node) => node.health == LavalinkHealth.healthy)
        .length;
    return [
      nodes.length == 1 ? '1 node' : '${nodes.length} nodes',
      '$healthy healthy',
      channelsPlaying == 1
          ? '1 channel playing'
          : '$channelsPlaying channels playing',
    ].join(' · ');
  }

  /// Every enabled node is unavailable.
  bool get allDown {
    final enabled = nodes.where((node) => node.settings.enabled);
    return enabled.isNotEmpty &&
        enabled.every((node) => node.health == LavalinkHealth.unavailable);
  }

  /// With [node] in place of the one with its id, or added.
  LavalinkNodes withNode(LavalinkNode node, {int? channelsPlaying}) {
    final known = this.node(node.id) != null;
    return LavalinkNodes(
      nodes: [
        for (final old in nodes) old.id == node.id ? node : old,
        if (!known) node,
      ],
      bridge: bridge,
      events: events,
      channelsPlaying: channelsPlaying ?? this.channelsPlaying,
    );
  }

  LavalinkNodes without(int id, {int? channelsPlaying}) => LavalinkNodes(
    nodes: [
      for (final node in nodes)
        if (node.id != id) node,
    ],
    bridge: bridge,
    events: events,
    channelsPlaying: channelsPlaying ?? this.channelsPlaying,
  );

  /// With [event] first; an event a reply already brought is not added
  /// twice when it also arrives on its own.
  LavalinkNodes withEvent(LavalinkNodeEvent event) => events.contains(event)
      ? this
      : LavalinkNodes(
          nodes: nodes,
          bridge: bridge,
          events: [event, ...events].take(lavalinkEventsKept).toList(),
          channelsPlaying: channelsPlaying,
        );

  @override
  bool operator ==(Object other) =>
      other is LavalinkNodes &&
      listEquals(other.nodes, nodes) &&
      other.bridge == bridge &&
      listEquals(other.events, events) &&
      other.channelsPlaying == channelsPlaying;

  @override
  int get hashCode => Object.hash(
    Object.hashAll(nodes),
    bridge,
    Object.hashAll(events),
    channelsPlaying,
  );
}

/// Changes to a node; what is null stays.
@immutable
class LavalinkNodeChanges {
  const LavalinkNodeChanges({this.settings, this.password, this.draining});

  final LavalinkNodeSettings? settings;
  final String? password;
  final bool? draining;
}

enum LavalinkTestStepKind {
  rest('Reach Lavalink and sign in'),
  websocket('Open its WebSocket'),
  bridge('Lavalink reaches this server');

  const LavalinkTestStepKind(this.label);

  final String label;
}

enum LavalinkTestStepState { waiting, running, passed, failed, skipped }

@immutable
class LavalinkTestStep {
  const LavalinkTestStep({
    required this.kind,
    this.state = LavalinkTestStepState.waiting,
    this.detail = '',
  });

  final LavalinkTestStepKind kind;
  final LavalinkTestStepState state;

  /// What passed, or what failed and how to fix it.
  final String detail;

  @override
  bool operator ==(Object other) =>
      other is LavalinkTestStep &&
      other.kind == kind &&
      other.state == state &&
      other.detail == detail;

  @override
  int get hashCode => Object.hash(kind, state, detail);
}

@immutable
class LavalinkTestProgress {
  const LavalinkTestProgress({
    required this.nodeId,
    required this.steps,
    this.done = false,
  });

  /// Before the server's first word: every step waiting.
  factory LavalinkTestProgress.started(int nodeId) => LavalinkTestProgress(
    nodeId: nodeId,
    steps: [
      for (final kind in LavalinkTestStepKind.values)
        LavalinkTestStep(kind: kind),
    ],
  );

  final int nodeId;
  final List<LavalinkTestStep> steps;
  final bool done;

  bool get passed =>
      done && steps.every((step) => step.state == LavalinkTestStepState.passed);

  @override
  bool operator ==(Object other) =>
      other is LavalinkTestProgress &&
      other.nodeId == nodeId &&
      listEquals(other.steps, steps) &&
      other.done == done;

  @override
  int get hashCode => Object.hash(nodeId, Object.hashAll(steps), done);
}
