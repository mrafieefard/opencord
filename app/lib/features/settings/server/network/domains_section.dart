import 'package:flutter/material.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/add_domain_dialog.dart';
import 'package:opencord/features/settings/server/network/domain_row.dart';
import 'package:opencord/features/settings/server/network/free_domain_dialog.dart';
import 'package:opencord/features/settings/server/network/tunnel_dialog.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// The server's domains (Phase 3 plan §6, §8, §9): each with its status,
/// and ways to add one: an own domain or a proxy's, a free subdomain, or a
/// Cloudflare Tunnel.
class DomainsSection extends StatelessWidget {
  const DomainsSection({
    super.key,
    required this.serverKey,
    required this.domains,
    this.tunnelOn = false,
  });

  final String serverKey;
  final List<Domain> domains;

  /// A tunnel runs: it is stopped before another starts.
  final bool tunnelOn;

  static const maxDomains = 20;

  @override
  Widget build(BuildContext context) {
    final full = domains.length >= maxDomains;
    return SettingsSection(
      title: 'Domains',
      footer:
          'People connect through the primary domain, and the others when it '
          'does not answer. Domains are checked again every 12 hours; one '
          'that breaks stays active, and you are told.',
      children: [
        for (final domain in domains)
          DomainRow(
            key: ValueKey(domain.id),
            serverKey: serverKey,
            domain: domain,
          ),
        SettingsRow(
          icon: OcIcons.add,
          title: domains.isEmpty ? 'No domains yet' : 'Add another domain',
          subtitle: full
              ? 'A server has at most $maxDomains domains'
              : 'A name people connect by instead of the IP address',
          trailing: OcButton(
            label: 'Add domain',
            dense: true,
            onPressed: full
                ? null
                : () => showOcDialog<void>(
                    context: context,
                    builder: (_) => AddDomainDialog(serverKey: serverKey),
                  ),
          ),
        ),
        SettingsRow(
          icon: OcIcons.link,
          title: 'No domain?',
          subtitle:
              'A free name from DuckDNS or deSEC, kept pointed at this '
              'server, with its certificate',
          trailing: OcButton(
            label: 'Get a free domain',
            dense: true,
            onPressed: full
                ? null
                : () => showOcDialog<void>(
                    context: context,
                    builder: (_) => FreeDomainDialog(serverKey: serverKey),
                  ),
          ),
        ),
        SettingsRow(
          icon: OcIcons.sync,
          title: 'Ports cannot be opened?',
          subtitle:
              'A Cloudflare Tunnel reaches this server without open ports; '
              'voice still needs UDP',
          trailing: OcButton(
            label: 'Use a tunnel',
            dense: true,
            onPressed: full || tunnelOn
                ? null
                : () => showOcDialog<void>(
                    context: context,
                    builder: (_) => TunnelDialog(serverKey: serverKey),
                  ),
          ),
        ),
      ],
    );
  }
}
