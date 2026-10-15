import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/music/node_dialog.dart';
import 'package:opencord/features/settings/server/music/node_test_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Sends a node change and shows what went wrong, if anything.
Future<void> changeLavalink(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Future<LavalinkNodes> Function(OpencordRepository repository, String server)
  action, {
  String? done,
}) async {
  try {
    await ref.read(lavalinkProvider(serverKey).notifier).change(action);
    if (done != null && context.mounted) showOcToast(context, done);
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
  }
}

/// Lavalink nodes (Phase 3 plan §12.8): what Lavalink needs from this
/// server, one row per node, and what happened to them.
class LavalinkSection extends ConsumerWidget {
  const LavalinkSection({
    super.key,
    required this.serverKey,
    required this.nodes,
  });

  final String serverKey;
  final LavalinkNodes nodes;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final full = nodes.nodes.length >= maxLavalinkNodes;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (nodes.allDown) ...[
          const InlineError(
            'Music server unavailable, retrying… Every node is unavailable, '
            'so music is paused and queues are kept.',
          ),
          const SizedBox(height: OcSpace.s16),
        ],
        BridgeChecklist(bridge: nodes.bridge),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Lavalink nodes',
          footer: nodes.nodes.isEmpty
              ? null
              : '${nodes.summary}. Opencord plays each channel on the best '
                    'healthy node and moves channels when one goes down. '
                    'Give every node the same sources, so tracks survive a '
                    'move.',
          children: [
            for (final node in nodes.nodes)
              LavalinkNodeRow(
                key: ValueKey(node.id),
                serverKey: serverKey,
                nodeId: node.id,
              ),
            SettingsRow(
              icon: OcIcons.add,
              title: nodes.nodes.isEmpty ? 'No nodes yet' : 'Add another node',
              subtitle: full
                  ? 'A server has at most $maxLavalinkNodes nodes'
                  : 'Music plays on Lavalink: your own, or a public one',
              trailing: OcButton(
                label: 'Add node',
                dense: true,
                onPressed: full
                    ? null
                    : () => showOcDialog<void>(
                        context: context,
                        builder: (_) =>
                            LavalinkNodeDialog(serverKey: serverKey),
                      ),
              ),
            ),
          ],
        ),
        if (nodes.events.isNotEmpty) ...[
          const SizedBox(height: OcSpace.s24),
          NodeEventsSection(nodes: nodes),
        ],
      ],
    );
  }
}

/// What Lavalink needs: where it connects, where it sends audio, and the
/// router's mapping.
class BridgeChecklist extends StatelessWidget {
  const BridgeChecklist({super.key, required this.bridge});

  final LavalinkBridge bridge;

  @override
  Widget build(BuildContext context) {
    if (!bridge.enabled) {
      return const SettingsSection(
        title: 'What Lavalink needs',
        children: [
          SettingsRow(
            icon: OcIcons.warning,
            title: 'Music is off on this server',
            subtitle:
                'Turn it on with [music] enabled in opencord.toml; voice must '
                'run on this server.',
          ),
        ],
      );
    }
    return SettingsSection(
      title: 'What Lavalink needs',
      footer:
          'Lavalink connects back to this server to deliver the audio. A '
          'public node needs a domain with a trusted certificate and the UDP '
          'port open.',
      children: [
        SettingsRow(
          icon: bridge.trustedCertificate ? OcIcons.check : OcIcons.warning,
          title: 'Lavalink connects to ${bridge.url}',
          mono: true,
          subtitle: bridge.trustedCertificate
              ? 'Trusted certificate'
              : 'Self-signed certificate: only a Lavalink that imported it '
                    'can connect. Activate a domain with a certificate in '
                    'Domains & network.',
        ),
        SettingsRow(
          icon: bridge.audioListening && bridge.audioAddress.isNotEmpty
              ? OcIcons.check
              : OcIcons.warning,
          title: bridge.audioAddress.isEmpty
              ? 'Sends audio to UDP port ${bridge.audioPort}'
              : 'Sends audio to UDP ${bridge.audioAddress}',
          subtitle: [
            bridge.audioListening ? 'Listening' : 'Not listening',
            if (bridge.audioAddress.isEmpty)
              'the public address is not known yet; nodes on this machine or '
                  'network still work',
          ].join(' · '),
        ),
        SettingsRow(
          icon: switch (bridge.mapping) {
            MappingState.failed => OcIcons.warning,
            MappingState.mapped => OcIcons.check,
            MappingState.notUsed => OcIcons.remove,
          },
          title: 'Router mapping',
          subtitle: switch (bridge.mapping) {
            MappingState.mapped => 'UPnP opened the port',
            MappingState.failed => 'UPnP could not open the port',
            MappingState.notUsed => 'Not used',
          },
        ),
      ],
    );
  }
}

/// One node: its state, address, load and sources, with Test, the Enabled
/// switch and a menu. It rebuilds only when that node changes.
class LavalinkNodeRow extends ConsumerWidget {
  const LavalinkNodeRow({
    super.key,
    required this.serverKey,
    required this.nodeId,
  });

  final String serverKey;
  final int nodeId;

  Future<void> _update(
    BuildContext context,
    WidgetRef ref,
    LavalinkNodeChanges changes, {
    String? done,
  }) => changeLavalink(
    context,
    ref,
    serverKey,
    (repository, server) =>
        repository.updateLavalinkNode(server, nodeId, changes),
    done: done,
  );

  Future<void> _delete(
    BuildContext context,
    WidgetRef ref,
    LavalinkNode node,
  ) async {
    final name = node.settings.name;
    final confirmed = await confirmAction(
      context,
      title: 'Delete $name?',
      message: node.channels == 0
          ? 'Opencord stops using it.'
          : 'The ${node.channels} channels playing on it move to another '
                'node first.',
      action: 'Delete node',
    );
    if (!confirmed || !context.mounted) return;
    await changeLavalink(
      context,
      ref,
      serverKey,
      (repository, server) => repository.deleteLavalinkNode(server, nodeId),
      done: '$name deleted',
    );
  }

  void _menu(BuildContext anchor, WidgetRef ref, LavalinkNode node) {
    final name = node.settings.name;
    showOcMenu(
      context: anchor,
      position: globalRectOf(anchor).bottomLeft.translate(0, 4),
      entries: [
        OcMenuItem(
          label: 'Edit',
          icon: OcIcons.edit,
          onSelected: () => showOcDialog<void>(
            context: anchor,
            builder: (_) =>
                LavalinkNodeDialog(serverKey: serverKey, node: node),
          ),
        ),
        if (node.settings.enabled)
          OcMenuItem(
            label: node.draining ? 'Undrain' : 'Drain',
            icon: node.draining ? OcIcons.playArrow : OcIcons.moveDown,
            onSelected: () => _update(
              anchor,
              ref,
              LavalinkNodeChanges(draining: !node.draining),
              done: node.draining
                  ? '$name gets new channels again'
                  : '$name gets no new channels',
            ),
          ),
        if (node.channels > 0)
          OcMenuItem(
            label: 'Move players now',
            icon: OcIcons.sync,
            onSelected: () => changeLavalink(
              anchor,
              ref,
              serverKey,
              (repository, server) =>
                  repository.moveLavalinkPlayers(server, nodeId),
              done: 'Moving the channels on $name',
            ),
          ),
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Delete',
          icon: OcIcons.delete,
          onSelected: () => _delete(anchor, ref, node),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final node = ref.watch(lavalinkNodeProvider((serverKey, nodeId)));
    if (node == null) return const SizedBox.shrink();
    final settings = node.settings;
    final health = node.recovering
        ? 'Back, waiting 30 s before new channels'
        : node.health.label;
    final lines = <String>[
      [
        settings.address,
        if (settings.secure) 'TLS',
        if (node.version.isNotEmpty) 'Lavalink ${node.version}',
        health,
        if (settings.priority != LavalinkPriority.normal)
          settings.priority.label,
      ].join(' · '),
      if (node.reason.isNotEmpty) node.reason,
      if (node.health.isUp)
        [
          switch (node.channels) {
            1 => '1 channel',
            final count => '$count channels',
          },
          '${node.players} players',
          'CPU ${node.cpuPercent} %',
          if (node.pingMs > 0) '${node.pingMs} ms',
        ].join(' · '),
      if (node.health.isUp && node.sources.isNotEmpty)
        'Sources: ${node.sources.join(', ')}',
      if (node.reason.isEmpty &&
          node.lastError.isNotEmpty &&
          node.health != LavalinkHealth.disabled)
        'Last error: ${node.lastError}',
    ];
    final icon = switch (node.health) {
      LavalinkHealth.healthy => OcIcons.check,
      LavalinkHealth.degraded => OcIcons.warning,
      LavalinkHealth.unavailable => OcIcons.error,
      LavalinkHealth.draining => OcIcons.moveDown,
      LavalinkHealth.disabled => OcIcons.remove,
      LavalinkHealth.connecting => OcIcons.hourglassEmpty,
    };
    return SettingsRow(
      icon: icon,
      title: settings.name,
      subtitle: lines.join('\n'),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          OcButton(
            label: 'Test',
            dense: true,
            onPressed: settings.enabled
                ? () => showOcDialog<void>(
                    context: context,
                    builder: (_) =>
                        LavalinkTestDialog(serverKey: serverKey, node: node),
                  )
                : null,
          ),
          const SizedBox(width: OcSpace.s12),
          Tooltip(
            message: settings.enabled ? 'Enabled' : 'Disabled',
            child: Semantics(
              label: 'Enabled',
              toggled: settings.enabled,
              child: OcSwitch(
                value: settings.enabled,
                onChanged: (on) => _update(
                  context,
                  ref,
                  LavalinkNodeChanges(settings: settings.copyWith(enabled: on)),
                  done: on ? '${settings.name} enabled' : null,
                ),
              ),
            ),
          ),
          const SizedBox(width: OcSpace.s4),
          Builder(
            builder: (anchor) => OcIconButton(
              icon: OcIcons.moreHoriz,
              tooltip: 'More for ${settings.name}',
              size: OcIconButtonSize.compact,
              onPressed: () => _menu(anchor, ref, node),
            ),
          ),
        ],
      ),
    );
  }
}

/// The last node events, newest first, in a list that stays short.
class NodeEventsSection extends StatelessWidget {
  const NodeEventsSection({super.key, required this.nodes});

  final LavalinkNodes nodes;

  static const _rowHeight = 22.0;
  static const _visibleRows = 8;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final now = clock.now();
    final events = nodes.events;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SectionLabel(
          'Node events',
          padding: EdgeInsets.only(left: 4, bottom: OcSpace.s8),
        ),
        SizedBox(
          height:
              _rowHeight *
              (events.length < _visibleRows ? events.length : _visibleRows),
          child: ListView.builder(
            itemCount: events.length,
            itemExtent: _rowHeight,
            itemBuilder: (context, index) {
              final event = events[index];
              return Row(
                children: [
                  SizedBox(
                    width: 72,
                    child: Text(
                      rowTime(event.at, now),
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  ),
                  Expanded(
                    child: Text(
                      event.text,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}
