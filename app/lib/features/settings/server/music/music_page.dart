import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/music/lavalink_section.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

/// Server settings → Music (Phase 3 plan §12.8, §16.2). For now its
/// Lavalink nodes, live while the page is open.
class ServerMusicPage extends ConsumerWidget {
  const ServerMusicPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final nodes = ref.watch(lavalinkProvider(serverKey));
    return switch (nodes) {
      AsyncData(:final value) => LavalinkSection(
        serverKey: serverKey,
        nodes: value,
      ),
      AsyncError(:final error) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InlineError(
            error is RepoException
                ? error.message
                : 'The music settings could not be loaded.',
          ),
          const SizedBox(height: OcSpace.s12),
          OcButton(
            label: 'Try again',
            onPressed: () =>
                ref.read(lavalinkProvider(serverKey).notifier).retry(),
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
