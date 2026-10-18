import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/network/add_domain_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// What tunnels cannot carry, said wherever a tunnel or proxy is set up.
String udpNote(NetworkInfo? network) {
  int port(PortPurpose purpose, int fallback) =>
      switch (network?.port(purpose)) {
        final status? when status.port != 0 => status.port,
        _ => fallback,
      };
  return 'Tunnels and proxies carry no UDP: voice and video (UDP '
      '${port(PortPurpose.voice, 7711)}) and music from Lavalink (UDP '
      '${port(PortPurpose.music, 7712)}) still need to reach this server '
      'directly. Text works fully without them.';
}

enum _Choice { quick, named, own }

/// Choosing and starting a Cloudflare Tunnel (Phase 3 plan §9.2): quick
/// (for testing), named (with the owner's token), or one the owner runs.
class TunnelDialog extends ConsumerStatefulWidget {
  const TunnelDialog({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<TunnelDialog> createState() => _TunnelDialogState();
}

class _TunnelDialogState extends ConsumerState<TunnelDialog> {
  final _token = TextEditingController();
  final _hostname = TextEditingController();
  var _choice = _Choice.quick;
  var _busy = false;
  String? _error;

  @override
  void dispose() {
    _token.dispose();
    _hostname.dispose();
    super.dispose();
  }

  Future<void> _run(
    Future<NetworkInfo> Function(OpencordRepository repository, String server)
    action, {
    bool close = true,
  }) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref.read(networkProvider(widget.serverKey).notifier).change(action);
      if (!mounted) return;
      if (close) {
        Navigator.pop(context);
      } else {
        setState(() => _busy = false);
      }
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = error.message;
      });
    }
  }

  Future<void> _start() async {
    switch (_choice) {
      case _Choice.quick:
        await _run(
          (repository, server) =>
              repository.startTunnel(server, TunnelKind.quick),
        );
      case _Choice.named:
        await _run(
          (repository, server) => repository.startTunnel(
            server,
            TunnelKind.named,
            token: _token.text.trim(),
            hostname: _hostname.text.trim(),
          ),
        );
      case _Choice.own:
        Navigator.pop(context);
        await showOcDialog<void>(
          context: context,
          builder: (_) => AddDomainDialog(
            serverKey: widget.serverKey,
            initialKind: DomainKind.tunnel,
          ),
        );
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final network = ref.watch(networkProvider(widget.serverKey)).value;
    final tunnel = network?.tunnel ?? const TunnelStatus();
    final local = network?.settings.localListenerPort ?? 7719;
    final missing = !tunnel.hasCloudflared && _choice != _Choice.own;
    final ready = switch (_choice) {
      _Choice.quick => !missing,
      _Choice.named =>
        !missing &&
            _hostname.text.trim().isNotEmpty &&
            (_token.text.trim().isNotEmpty || tunnel.kind == TunnelKind.named),
      _Choice.own => true,
    };
    final muted = OcText.small.copyWith(color: colors.textSecondary);
    return OcDialog(
      title: 'Use a Cloudflare Tunnel',
      width: 560,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: _choice == _Choice.own ? 'Next' : 'Start tunnel',
          busy: _busy,
          onPressed: ready && !_busy ? _start : null,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text(
            'A tunnel reaches this server without open ports, when the '
            'router cannot forward them. Cloudflare provides the '
            'certificate.',
          ),
          const SizedBox(height: OcSpace.s16),
          ChoiceChips<_Choice>(
            options: const [
              (_Choice.quick, 'Quick tunnel'),
              (_Choice.named, 'My Cloudflare tunnel'),
              (_Choice.own, 'I run cloudflared'),
            ],
            value: _choice,
            onChanged: (choice) => setState(() {
              _choice = choice;
              _error = null;
            }),
          ),
          const SizedBox(height: OcSpace.s12),
          switch (_choice) {
            _Choice.quick => Text(
              'No Cloudflare account needed: you get a random '
              'trycloudflare.com address. For testing only: the address '
              'changes every time the tunnel starts, Cloudflare allows at '
              'most 200 requests at once, and there is no uptime guarantee.',
              style: muted,
            ),
            _Choice.named => Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'In Cloudflare\'s dashboard (Zero Trust → Networks → '
                  'Tunnels), create a tunnel and add a public hostname whose '
                  'service is http://localhost:$local. Then paste the '
                  'tunnel\'s token (the long value in its install command) '
                  'and that hostname.',
                  style: muted,
                ),
                const SizedBox(height: OcSpace.s12),
                OcTextField(
                  controller: _token,
                  mono: true,
                  hint: tunnel.kind == TunnelKind.named
                      ? 'Token saved: leave empty to keep it'
                      : 'Tunnel token',
                  semanticLabel: 'Tunnel token',
                  onChanged: (_) => setState(() => _error = null),
                ),
                const SizedBox(height: OcSpace.s8),
                OcTextField(
                  controller: _hostname,
                  mono: true,
                  hint: 'chat.example.com',
                  semanticLabel: 'Public hostname',
                  onChanged: (_) => setState(() => _error = null),
                ),
              ],
            ),
            _Choice.own => Text(
              'You run cloudflared yourself, for example as a Docker '
              'sidecar, forwarding a public hostname to '
              'http://localhost:$local on this machine. Add that hostname '
              'next; Opencord checks it through the tunnel.',
              style: muted,
            ),
          },
          if (missing) ...[
            const SizedBox(height: OcSpace.s16),
            const SectionLabel('cloudflared'),
            const SizedBox(height: OcSpace.s8),
            Row(
              children: [
                Expanded(
                  child: Text(
                    tunnel.installing
                        ? 'Downloading cloudflared…'
                        : 'cloudflared is not installed on the server. '
                              'Install it from Cloudflare, or let Opencord '
                              'download it and check its checksum.',
                    style: muted,
                  ),
                ),
                const SizedBox(width: OcSpace.s12),
                OcButton(
                  label: 'Download',
                  dense: true,
                  busy: tunnel.installing,
                  onPressed: tunnel.installing || _busy
                      ? null
                      : () => _run(
                          (repository, server) =>
                              repository.installCloudflared(server),
                          close: false,
                        ),
                ),
              ],
            ),
          ],
          const SizedBox(height: OcSpace.s16),
          Text(udpNote(network), style: muted),
          if (_error ?? (missing ? tunnel.error : '') case final error
              when error.isNotEmpty) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}
