import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/music/lavalink_section.dart';
import 'package:opencord/features/settings/server/music/local_songs_section.dart';
import 'package:opencord/features/settings/server/music/music_blocks_section.dart';
import 'package:opencord/features/settings/server/music/music_settings_section.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

/// Server settings → Music (Phase 3 plan §12.8, §16.2): the music settings
/// and blocked members for members who manage music, uploaded songs for
/// administrators (design 2026-10-24), the Lavalink nodes (live while the
/// page is open) for those who manage the server.
class ServerMusicPage extends ConsumerWidget {
  const ServerMusicPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final (manageMusic, manageServer, admin) = ref.watch(
      serverProvider(serverKey).select(
        (s) => (
          s.data?.can(Permissions.musicManage) ?? false,
          s.data?.can(Permissions.manageServer) ?? false,
          s.data?.can(Permissions.administrator) ?? false,
        ),
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (manageMusic) ...[
          _Loaded(
            value: ref.watch(musicSettingsProvider(serverKey)),
            what: 'The music settings',
            retry: () =>
                ref.read(musicSettingsProvider(serverKey).notifier).retry(),
            builder: (settings) =>
                MusicSettingsSection(serverKey: serverKey, saved: settings),
          ),
          const SizedBox(height: OcSpace.s24),
          MusicBlocksSection(serverKey: serverKey),
        ],
        if (admin) ...[
          const SizedBox(height: OcSpace.s24),
          LocalSongsSection(serverKey: serverKey),
        ],
        if (manageMusic && manageServer) const SizedBox(height: OcSpace.s32),
        if (manageServer)
          _Loaded(
            value: ref.watch(lavalinkProvider(serverKey)),
            what: 'The Lavalink nodes',
            retry: () => ref.read(lavalinkProvider(serverKey).notifier).retry(),
            builder: (nodes) =>
                LavalinkSection(serverKey: serverKey, nodes: nodes),
          ),
      ],
    );
  }
}

/// What loaded, its error with Try again, or a spinner.
class _Loaded<T> extends StatelessWidget {
  const _Loaded({
    required this.value,
    required this.what,
    required this.retry,
    required this.builder,
  });

  final AsyncValue<T> value;
  final String what;
  final VoidCallback retry;
  final Widget Function(T value) builder;

  @override
  Widget build(BuildContext context) => switch (value) {
    AsyncData(:final value) => builder(value),
    AsyncError(:final error) => Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InlineError(
          error is RepoException ? error.message : '$what could not be loaded.',
        ),
        const SizedBox(height: OcSpace.s12),
        OcButton(label: 'Try again', onPressed: retry),
      ],
    ),
    _ => const Padding(
      padding: EdgeInsets.all(OcSpace.s24),
      child: Center(child: OcSpinner(size: 20)),
    ),
  };
}
