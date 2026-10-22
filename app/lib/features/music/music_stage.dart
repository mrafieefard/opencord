import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_queue.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// Expand (Phase 3 plan §19.7): the music panel's widgets in the voice
/// view's main area, activity-style: the cover and controls in the middle,
/// Up next beside them. The tiles go to a strip below.
class MusicStage extends ConsumerWidget {
  const MusicStage({super.key, required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final queued = ref.watch(
      musicSessionProvider(
        channel,
      ).select((value) => value.value?.queue.isNotEmpty ?? false),
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        final beside = constraints.maxWidth >= 760 && queued;
        final cover = (constraints.maxHeight - 220).clamp(120.0, 360.0);
        return Stack(
          children: [
            Row(
              children: [
                Expanded(
                  child: Center(
                    child: SingleChildScrollView(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 420),
                        child: NowPlayingView(
                          channel: channel,
                          coverSize: cover,
                        ),
                      ),
                    ),
                  ),
                ),
                if (beside) ...[
                  VerticalDivider(width: 1, color: colors.border),
                  SizedBox(
                    width: 340,
                    child: CustomScrollView(
                      slivers: [
                        SliverPadding(
                          padding: const EdgeInsets.all(OcSpace.s12),
                          sliver: SliverToBoxAdapter(
                            child: Text(
                              'Up next',
                              style: OcText.label.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                          ),
                        ),
                        SliverPadding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: OcSpace.s8,
                          ),
                          sliver: MusicQueueList(channel: channel),
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
            Positioned(
              top: 0,
              right: 0,
              child: OcIconButton(
                icon: OcIcons.closeFullscreen,
                tooltip: 'Collapse',
                size: OcIconButtonSize.compact,
                onPressed: () =>
                    ref.read(musicExpandedProvider.notifier).set(false),
              ),
            ),
          ],
        );
      },
    );
  }
}
