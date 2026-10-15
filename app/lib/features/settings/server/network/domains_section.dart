import 'package:flutter/material.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/add_domain_dialog.dart';
import 'package:opencord/features/settings/server/network/domain_row.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// The server's domains (Phase 3 plan §6, §8): each with its status, and
/// a way to add one. Free subdomains and tunnels come with their own flows.
class DomainsSection extends StatelessWidget {
  const DomainsSection({
    super.key,
    required this.serverKey,
    required this.domains,
  });

  final String serverKey;
  final List<Domain> domains;

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
      ],
    );
  }
}
