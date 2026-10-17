import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// The ports and how they are reached (Phase 3 plan §5.2): each one's
/// state, the router's mapping with UPnP, and the main port.
class PortsSection extends ConsumerWidget {
  const PortsSection({super.key, required this.serverKey, required this.info});

  final String serverKey;
  final NetworkInfo info;

  Future<void> _changeMainPort(BuildContext context, WidgetRef ref) async {
    final port = await askPort(
      context,
      title: 'Main port',
      body:
          'Chat, voice signalling, invites and Lavalink all use this port. '
          'Use 443 so addresses and invite links need no port number. '
          'It changes when the server restarts.',
      current: info.settings.mainPort == 0
          ? info.listeningPort
          : info.settings.mainPort,
    );
    if (port == null || !context.mounted) return;
    await changeNetwork(
      context,
      ref,
      serverKey,
      (repository, server) => repository.updateNetworkSettings(
        server,
        NetworkSettingsChanges(mainPort: port),
      ),
      done: port == info.listeningPort
          ? null
          : 'Port $port is used from the next start',
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = info.settings;
    final upnpLine = switch ((settings.upnpEnabled, info.upnpGateway)) {
      (false, _) => 'For home servers: asks the router to forward the ports',
      (true, '') when info.upnpError.isNotEmpty => info.upnpError,
      (true, '') => 'Looking for the router…',
      (true, final gateway) =>
        info.upnpExternalIp.isEmpty
            ? 'Router $gateway'
            : 'Router $gateway · public address ${info.upnpExternalIp}',
    };
    return SettingsSection(
      title: 'Ports',
      footer:
          'Forward these to this machine on your router, or turn on UPnP. '
          'Voice and music need their UDP ports reachable directly: '
          'tunnels do not carry UDP.',
      children: [
        for (final port in info.ports) _PortRow(port: port),
        SettingsSwitchRow(
          title: 'Open ports on my router (UPnP)',
          subtitle: upnpLine,
          value: settings.upnpEnabled,
          onChanged: (on) => changeNetwork(
            context,
            ref,
            serverKey,
            (repository, server) => repository.updateNetworkSettings(
              server,
              NetworkSettingsChanges(upnpEnabled: on),
            ),
          ),
        ),
        SettingsRow(
          title: 'Main port',
          subtitle: info.restartPending
              ? 'Port ${settings.mainPort} from the next start'
                    '${_nextPortProblem(info.nextMainPort)}'
              : 'TCP ${info.listeningPort}',
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (settings.mainPort != 0) ...[
                OcButton.ghost(
                  label: 'Reset',
                  dense: true,
                  onPressed: () => changeNetwork(
                    context,
                    ref,
                    serverKey,
                    (repository, server) => repository.updateNetworkSettings(
                      server,
                      const NetworkSettingsChanges(mainPort: 0),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
              ],
              OcButton(
                label: 'Change',
                dense: true,
                onPressed: () => _changeMainPort(context, ref),
              ),
            ],
          ),
        ),
      ],
    );
  }

  static String _nextPortProblem(PortState state) => switch (state) {
    PortState.inUse => ', but another program has it now',
    PortState.notPermitted =>
      ", but this system does not let Opencord use it yet: see the guide's "
          'section on low ports',
    _ => '',
  };
}

class _PortRow extends StatelessWidget {
  const _PortRow({required this.port});

  final PortStatus port;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final protocol = port.protocol == PortProtocol.tcp ? 'TCP' : 'UDP';
    final number = port.port == 0 ? '' : ' ${port.port}';
    final router = switch (port.mapping) {
      MappingState.notUsed => null,
      MappingState.mapped => 'Router: forwarded',
      MappingState.failed => 'Router: ${port.mappingError}',
    };
    final (icon, fill) = switch (port.state) {
      PortState.listening => (OcIcons.check, 1.0),
      PortState.free || PortState.off => (OcIcons.remove, 1.0),
      PortState.inUse || PortState.notPermitted => (OcIcons.warning, 0.0),
      PortState.unknown => (OcIcons.info, 0.0),
    };
    return SettingsRow(
      title: '${port.purpose.label} · $protocol$number',
      subtitle: [port.purpose.description, ?router].join('\n'),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: OcSize.iconRow, fill: fill, color: colors.text),
          const SizedBox(width: OcSpace.s8),
          Text(
            port.state.label,
            style: OcText.small.copyWith(color: colors.textSecondary),
          ),
        ],
      ),
    );
  }
}
