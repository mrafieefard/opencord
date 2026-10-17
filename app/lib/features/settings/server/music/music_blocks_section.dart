import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Members blocked from music, with why and an Unblock button (Phase 3
/// plan §16.2). Blocking is in the member menu.
class MusicBlocksSection extends ConsumerWidget {
  const MusicBlocksSection({super.key, required this.serverKey});

  final String serverKey;

  Future<void> _unblock(
    BuildContext context,
    WidgetRef ref,
    MusicBlock block,
    String name,
  ) async {
    final unblock = await confirmAction(
      context,
      title: 'Unblock $name?',
      message: 'They can add tracks and control the music again.',
      action: 'Unblock',
    );
    if (!unblock) return;
    try {
      await ref
          .read(musicBlocksProvider(serverKey).notifier)
          .setBlocked(block.userId, blocked: false);
      if (context.mounted) showOcToast(context, '$name is unblocked');
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final blocks = ref.watch(musicBlocksProvider(serverKey));
    final members =
        ref.watch(serverProvider(serverKey).select((s) => s.data?.members)) ??
        const {};
    String nameOf(int userId) => members[userId]?.displayName ?? 'Someone';
    return SettingsSection(
      title: 'Blocked members',
      footer:
          'Blocked members can listen, but not add tracks or control the '
          'music. Block someone from their member menu.',
      children: switch (blocks) {
        AsyncData(value: final blocks) when blocks.isEmpty => [
          Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: Text(
              'No one is blocked from music.',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ),
        ],
        AsyncData(value: final blocks) => [
          for (final block in blocks)
            Padding(
              key: ValueKey(block.userId),
              padding: const EdgeInsets.symmetric(
                horizontal: OcSpace.s16,
                vertical: OcSpace.s10,
              ),
              child: Row(
                children: [
                  OcAvatar(
                    id: '${block.userId}',
                    name: nameOf(block.userId),
                    size: 32,
                  ),
                  const SizedBox(width: OcSpace.s12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          nameOf(block.userId),
                          style: OcText.body.copyWith(color: colors.text),
                        ),
                        Text(
                          [
                            'Since ${longDate(block.at)} by '
                                '${nameOf(block.blockedBy)}',
                            if (block.reason.isNotEmpty) block.reason,
                          ].join(' · '),
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.small.copyWith(color: colors.textMuted),
                        ),
                      ],
                    ),
                  ),
                  OcButton(
                    label: 'Unblock',
                    dense: true,
                    onPressed: () =>
                        _unblock(context, ref, block, nameOf(block.userId)),
                  ),
                ],
              ),
            ),
        ],
        AsyncError(:final error) => [
          Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: InlineError(
              error is RepoException
                  ? error.message
                  : 'The blocked members could not be loaded.',
            ),
          ),
        ],
        _ => [
          const Padding(
            padding: EdgeInsets.all(OcSpace.s16),
            child: Center(child: OcSpinner(size: 16)),
          ),
        ],
      },
    );
  }
}
