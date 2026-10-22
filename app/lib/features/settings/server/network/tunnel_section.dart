import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/features/settings/server/network/tunnel_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// The Cloudflare Tunnel while one runs (Phase 3 plan §9.2): its state,
/// address and restarts, cloudflared's output, and Stop.
class TunnelSection extends ConsumerWidget {
  const TunnelSection({super.key, required this.serverKey, required this.info});

  final String serverKey;
  final NetworkInfo info;

  Future<void> _stop(BuildContext context, WidgetRef ref) async {
    final tunnel = info.tunnel;
    final confirmed = await confirmAction(
      context,
      title: 'Stop the tunnel?',
      message: tunnel.kind == TunnelKind.quick
          ? 'People can no longer connect through ${tunnel.hostname}, and '
                'the address is gone: the next quick tunnel gets another.'
          : 'People can no longer connect through ${tunnel.hostname} until '
                'it runs again. The domain stays.',
      action: 'Stop tunnel',
    );
    if (!confirmed || !context.mounted) return;
    await changeNetwork(
      context,
      ref,
      serverKey,
      (repository, server) => repository.stopTunnel(server),
      done: 'Tunnel stopped',
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tunnel = info.tunnel;
    final kind = tunnel.kind == TunnelKind.quick
        ? 'Quick tunnel'
        : 'Named tunnel';
    final lines = [
      [
        kind,
        tunnel.state.label,
        if (tunnel.restarts > 0)
          tunnel.restarts == 1
              ? 'started again once'
              : 'started again ${tunnel.restarts} times',
      ].join(' · '),
      if (tunnel.error.isNotEmpty) tunnel.error,
      if (tunnel.kind == TunnelKind.quick)
        'For testing: the address changes whenever the tunnel starts, and '
            'Cloudflare allows 200 requests at once.',
    ];
    final busy =
        tunnel.state == TunnelState.starting ||
        tunnel.state == TunnelState.restarting;
    return SettingsSection(
      title: 'Cloudflare Tunnel',
      footer: udpNote(info),
      children: [
        SettingsRow(
          icon: tunnel.state == TunnelState.failed
              ? OcIcons.warning
              : OcIcons.sync,
          title: tunnel.hostname.isEmpty
              ? 'Waiting for an address'
              : tunnel.hostname,
          subtitle: lines.join('\n'),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (busy)
                const Padding(
                  padding: EdgeInsets.symmetric(horizontal: OcSpace.s12),
                  child: OcSpinner(),
                ),
              OcButton(
                label: 'Output',
                dense: true,
                onPressed: () => showOcDialog<void>(
                  context: context,
                  builder: (_) => TunnelLogDialog(serverKey: serverKey),
                ),
              ),
              const SizedBox(width: OcSpace.s8),
              OcButton(
                label: 'Stop',
                dense: true,
                onPressed: () => _stop(context, ref),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// cloudflared's last lines, as the server kept them.
class TunnelLogDialog extends ConsumerWidget {
  const TunnelLogDialog({super.key, required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return OcDialog(
      title: 'cloudflared output',
      showClose: true,
      width: 640,
      child: FutureBuilder<List<String>>(
        future: ref.read(repositoryProvider).tunnelLog(serverKey),
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return const Text('The output could not be loaded.');
          }
          final lines = snapshot.data;
          if (lines == null) {
            return const Center(child: OcSpinner(size: 20));
          }
          if (lines.isEmpty) return const Text('Nothing yet.');
          return ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 360),
            child: ListView.builder(
              shrinkWrap: true,
              itemCount: lines.length,
              itemBuilder: (context, index) => SelectableText(
                lines[index],
                style: OcText.mono.copyWith(
                  color: colors.textSecondary,
                  fontSize: 12,
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
