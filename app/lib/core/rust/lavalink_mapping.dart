import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

/// The core's view of the Lavalink nodes as the app's model.
LavalinkNodes lavalinkNodesFrom(core.LavalinkNodes nodes) => LavalinkNodes(
  nodes: [for (final node in nodes.nodes) lavalinkNodeFrom(node)],
  bridge: _bridgeFrom(nodes.bridge),
  events: [for (final event in nodes.events) lavalinkEventFrom(event)],
  channelsPlaying: nodes.channelsPlaying,
);

LavalinkPriority _priorityFrom(core.LavalinkPriority priority) =>
    switch (priority) {
      core.LavalinkPriority.preferred => LavalinkPriority.preferred,
      core.LavalinkPriority.normal => LavalinkPriority.normal,
      core.LavalinkPriority.backup => LavalinkPriority.backup,
    };

LavalinkNodeSettings _settingsFrom(core.LavalinkNodeSettings settings) =>
    LavalinkNodeSettings(
      name: settings.name,
      host: settings.host,
      port: settings.port,
      secure: settings.secure,
      priority: _priorityFrom(settings.priority),
      maxPlayers: settings.maxPlayers,
      strictAddress: settings.strictAddress,
      enabled: settings.enabled,
    );

core.LavalinkNodeSettings lavalinkSettingsToCore(
  LavalinkNodeSettings settings,
) => core.LavalinkNodeSettings(
  name: settings.name,
  host: settings.host,
  port: settings.port,
  secure: settings.secure,
  priority: switch (settings.priority) {
    LavalinkPriority.preferred => core.LavalinkPriority.preferred,
    LavalinkPriority.normal => core.LavalinkPriority.normal,
    LavalinkPriority.backup => core.LavalinkPriority.backup,
  },
  maxPlayers: settings.maxPlayers,
  strictAddress: settings.strictAddress,
  enabled: settings.enabled,
);

LavalinkNode lavalinkNodeFrom(core.LavalinkNode node) => LavalinkNode(
  id: node.id,
  settings: _settingsFrom(node.settings),
  draining: node.draining,
  health: switch (node.health) {
    core.LavalinkHealth.connecting => LavalinkHealth.connecting,
    core.LavalinkHealth.healthy => LavalinkHealth.healthy,
    core.LavalinkHealth.degraded => LavalinkHealth.degraded,
    core.LavalinkHealth.unavailable => LavalinkHealth.unavailable,
    core.LavalinkHealth.draining => LavalinkHealth.draining,
    core.LavalinkHealth.disabled => LavalinkHealth.disabled,
  },
  reason: node.reason,
  version: node.version,
  sources: node.sources,
  plugins: node.plugins,
  channels: node.channels,
  players: node.players,
  playingPlayers: node.playingPlayers,
  cpuPercent: node.cpuPercent,
  pingMs: node.pingMs,
  lastError: node.lastError,
  connectedSince: node.connectedSinceMs == 0
      ? null
      : DateTime.fromMillisecondsSinceEpoch(node.connectedSinceMs),
  recovering: node.recovering,
);

LavalinkNodeEvent lavalinkEventFrom(core.LavalinkNodeEvent event) =>
    LavalinkNodeEvent(
      nodeId: event.nodeId,
      kind: switch (event.kind) {
        core.LavalinkEventKind.connected => LavalinkEventKind.connected,
        core.LavalinkEventKind.disconnected => LavalinkEventKind.disconnected,
        core.LavalinkEventKind.degraded => LavalinkEventKind.degraded,
        core.LavalinkEventKind.recovered => LavalinkEventKind.recovered,
        core.LavalinkEventKind.moved => LavalinkEventKind.moved,
      },
      text: event.text,
      at: DateTime.fromMillisecondsSinceEpoch(event.atMs),
    );

LavalinkBridge _bridgeFrom(core.LavalinkBridge bridge) => LavalinkBridge(
  enabled: bridge.enabled,
  url: bridge.url,
  trustedCertificate: bridge.trustedCertificate,
  audioAddress: bridge.audioAddress,
  audioPort: bridge.audioPort,
  audioListening: bridge.audioListening,
  mapping: switch (bridge.mapping) {
    core.MappingState.notUsed => MappingState.notUsed,
    core.MappingState.mapped => MappingState.mapped,
    core.MappingState.failed => MappingState.failed,
  },
);

LavalinkTestProgress lavalinkTestFrom(
  core.LavalinkTestProgress progress,
) => LavalinkTestProgress(
  nodeId: progress.nodeId,
  steps: [
    for (final step in progress.steps)
      LavalinkTestStep(
        kind: switch (step.kind) {
          core.LavalinkTestStepKind.rest => LavalinkTestStepKind.rest,
          core.LavalinkTestStepKind.websocket => LavalinkTestStepKind.websocket,
          core.LavalinkTestStepKind.bridge => LavalinkTestStepKind.bridge,
        },
        state: switch (step.state) {
          core.LavalinkTestStepState.waiting => LavalinkTestStepState.waiting,
          core.LavalinkTestStepState.running => LavalinkTestStepState.running,
          core.LavalinkTestStepState.passed => LavalinkTestStepState.passed,
          core.LavalinkTestStepState.failed => LavalinkTestStepState.failed,
          core.LavalinkTestStepState.skipped => LavalinkTestStepState.skipped,
        },
        detail: step.detail,
      ),
  ],
  done: progress.done,
);
