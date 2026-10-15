import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/network/advanced_section.dart';
import 'package:opencord/features/settings/server/network/domains_section.dart';
import 'package:opencord/features/settings/server/network/ports_section.dart';
import 'package:opencord/features/settings/server/network/public_address_section.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

/// Server settings → Domains & network (Phase 3 plan §8), the owner's
/// only. The server does the work; the page shows what it reports, live
/// while it is open.
class ServerNetworkPage extends ConsumerWidget {
  const ServerNetworkPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final network = ref.watch(networkProvider(serverKey));
    return switch (network) {
      AsyncData(:final value) => _Loaded(serverKey: serverKey, info: value),
      AsyncError(:final error) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InlineError(
            error is RepoException
                ? error.message
                : 'The network settings could not be loaded.',
          ),
          const SizedBox(height: OcSpace.s12),
          OcButton(
            label: 'Try again',
            onPressed: () =>
                ref.read(networkProvider(serverKey).notifier).retry(),
          ),
        ],
      ),
      _ => const Padding(
        padding: EdgeInsets.all(OcSpace.s24),
        child: Center(child: OcSpinner(size: 20)),
      ),
    };
  }
}

class _Loaded extends StatelessWidget {
  const _Loaded({required this.serverKey, required this.info});

  final String serverKey;
  final NetworkInfo info;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      PublicAddressSection(serverKey: serverKey, address: info.publicAddress),
      const SizedBox(height: OcSpace.s24),
      DomainsSection(serverKey: serverKey, domains: info.domains),
      const SizedBox(height: OcSpace.s24),
      PortsSection(serverKey: serverKey, info: info),
      const SizedBox(height: OcSpace.s24),
      AdvancedNetworkSection(serverKey: serverKey, settings: info.settings),
    ],
  );
}
