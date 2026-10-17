import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// What most owners never change (Phase 3 plan §8): the resolvers domain
/// checks use, Let's Encrypt's test environment, and the local listener
/// tunnels and proxies forward to.
class AdvancedNetworkSection extends ConsumerWidget {
  const AdvancedNetworkSection({
    super.key,
    required this.serverKey,
    required this.settings,
  });

  final String serverKey;
  final NetworkSettings settings;

  Future<void> _save(
    BuildContext context,
    WidgetRef ref,
    NetworkSettingsChanges changes,
  ) => changeNetwork(
    context,
    ref,
    serverKey,
    (repository, server) => repository.updateNetworkSettings(server, changes),
  );

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return SettingsSection(
      title: 'Advanced',
      children: [
        SettingsRow(
          title: 'DNS resolvers for checks',
          subtitle: settings.dnsResolvers.join(', '),
          trailing: OcButton(
            label: 'Change',
            dense: true,
            onPressed: () async {
              final resolvers = await askList(
                context,
                title: 'DNS resolvers',
                body:
                    'Domain checks find each domain\'s name servers through '
                    'these. Change them when your network blocks the '
                    'defaults. One address per line, with :port if not 53.',
                current: settings.dnsResolvers,
                hint: '1.1.1.1',
                allowEmpty: false,
              );
              if (resolvers == null || !context.mounted) return;
              await _save(
                context,
                ref,
                NetworkSettingsChanges(dnsResolvers: resolvers),
              );
            },
          ),
        ),
        SettingsSwitchRow(
          title: "Let's Encrypt test environment",
          subtitle:
              'For trying the setup without reaching its limits. Its '
              'certificates are not trusted and show as Test.',
          value: settings.acmeStaging,
          onChanged: (on) =>
              _save(context, ref, NetworkSettingsChanges(acmeStaging: on)),
        ),
        SettingsRow(
          title: 'Local listener port',
          subtitle:
              'Tunnels and reverse proxies on this machine forward to '
              'TCP ${settings.localListenerPort}, without TLS',
          trailing: OcButton(
            label: 'Change',
            dense: true,
            onPressed: () async {
              final port = await askPort(
                context,
                title: 'Local listener port',
                body:
                    'Only programs on this machine can reach it. It runs '
                    'while a tunnel or reverse-proxy domain exists.',
                current: settings.localListenerPort,
              );
              if (port == null || !context.mounted) return;
              await _save(
                context,
                ref,
                NetworkSettingsChanges(localListenerPort: port),
              );
            },
          ),
        ),
        SettingsRow(
          title: 'Trusted proxies',
          subtitle: settings.trustedProxies.isEmpty
              ? 'None'
              : settings.trustedProxies.join(', '),
          trailing: OcButton(
            label: 'Change',
            dense: true,
            onPressed: () async {
              final proxies = await askList(
                context,
                title: 'Trusted proxies',
                body:
                    'Connections from these addresses may say who they '
                    'forward for (X-Forwarded-For). Add your reverse proxy '
                    'if it runs on another machine. One address or range '
                    '(10.0.0.0/8) per line.',
                current: settings.trustedProxies,
                hint: '127.0.0.1',
              );
              if (proxies == null || !context.mounted) return;
              await _save(
                context,
                ref,
                NetworkSettingsChanges(trustedProxies: proxies),
              );
            },
          ),
        ),
      ],
    );
  }
}
