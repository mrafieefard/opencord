import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// DNS providers (Phase 3 plan §7.2, §8): tokens that let the server use
/// the DNS challenge, which needs no open ports. Tokens stay on the
/// server; only their last characters come back.
class DnsProvidersSection extends ConsumerWidget {
  const DnsProvidersSection({
    super.key,
    required this.serverKey,
    required this.providers,
  });

  final String serverKey;
  final List<DnsProvider> providers;

  static const _about = {
    DnsProviderKind.cloudflare:
        'For domains whose DNS Cloudflare runs. Make a token with "Edit '
        'zone DNS" permission for the zone.',
    DnsProviderKind.duckdns: 'For free subdomains under duckdns.org.',
    DnsProviderKind.desec: 'For domains and free subdomains at deSEC.',
  };

  Future<void> _connect(
    BuildContext context,
    WidgetRef ref,
    DnsProviderKind kind,
  ) async {
    final token = await showOcDialog<String>(
      context: context,
      builder: (_) => ValueDialog(
        title: 'Connect ${kind.label}',
        body:
            '${_about[kind]} The token is kept on the server, encrypted, and '
            'is only used to set the records certificates need.',
        initial: '',
        hint: kind.tokenLabel,
        action: 'Connect',
        check: (value) => value.isEmpty ? 'Paste the token first.' : null,
      ),
    );
    if (token == null || !context.mounted) return;
    await changeNetwork(
      context,
      ref,
      serverKey,
      (repository, server) => repository.setDnsProvider(server, kind, token),
      done: '${kind.label} connected',
    );
  }

  Future<void> _disconnect(
    BuildContext context,
    WidgetRef ref,
    DnsProviderKind kind,
  ) async {
    final confirmed = await confirmAction(
      context,
      title: 'Disconnect ${kind.label}?',
      message:
          'Its token is deleted from the server. Certificates that used the '
          'DNS challenge are renewed another way, if one works.',
      action: 'Disconnect',
    );
    if (!confirmed || !context.mounted) return;
    await changeNetwork(
      context,
      ref,
      serverKey,
      (repository, server) => repository.setDnsProvider(server, kind, ''),
      done: '${kind.label} disconnected',
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return SettingsSection(
      title: 'DNS providers',
      footer:
          'With a provider connected, certificates for its domains use the '
          'DNS challenge: no ports need to be open.',
      children: [
        for (final kind in DnsProviderKind.values)
          _row(
            context,
            ref,
            kind,
            providers.where((provider) => provider.kind == kind).firstOrNull,
          ),
      ],
    );
  }

  Widget _row(
    BuildContext context,
    WidgetRef ref,
    DnsProviderKind kind,
    DnsProvider? connected,
  ) => SettingsRow(
    icon: OcIcons.dns,
    title: kind.label,
    subtitle: connected == null
        ? _about[kind]
        : 'Connected · token ${connected.masked}',
    trailing: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        OcButton(
          label: connected == null ? 'Connect' : 'Replace',
          dense: true,
          onPressed: () => _connect(context, ref, kind),
        ),
        if (connected != null) ...[
          const SizedBox(width: OcSpace.s8),
          OcButton(
            label: 'Disconnect',
            dense: true,
            onPressed: () => _disconnect(context, ref, kind),
          ),
        ],
      ],
    ),
  );
}
