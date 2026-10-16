import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/features/music/music_artwork.dart';
import 'package:opencord/features/music/music_controls.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

/// What plays in the voice channel the right-side panel is about (§19.3):
/// only at the top of the users list, never as a participant. A click
/// opens the music panel; right-click has the listener's volume and mute.
class NowPlayingCard extends ConsumerWidget {
  const NowPlayingCard({super.key});

  void _menu(BuildContext context, WidgetRef ref, Offset position) {
    final listening = ref.read(musicListeningProvider);
    showOcMenu(
      context: context,
      position: position,
      entries: [
        OcMenuCustom((_) => const MusicVolumeControls()),
        const OcMenuDivider(),
        OcMenuItem(
          label: listening.muted ? 'Unmute music' : 'Mute music',
          icon: listening.muted ? OcIcons.volumeUp : OcIcons.volumeOff,
          onSelected: () => ref
              .read(musicListeningProvider.notifier)
              .update((l) => l.copyWith(muted: !l.muted)),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channel = ref.watch(musicChannelProvider);
    if (channel == null) return const SizedBox.shrink();
    final now = ref.watch(
      musicNowPlayingProvider(
        channel.server,
      ).select((playing) => playing[channel.channel]),
    );
    if (now == null) return const SizedBox.shrink();
    final colors = context.oc;
    final detail = [
      if (now.artist.isNotEmpty) now.artist,
      if (now.sourceName.isNotEmpty) now.sourceName,
    ].join(' · ');
    return Padding(
      padding: const EdgeInsets.fromLTRB(OcSpace.s8, OcSpace.s8, OcSpace.s8, 0),
      child: Builder(
        builder: (card) => Hoverable(
          onTap: () => ref
              .read(rightPanelModeProvider.notifier)
              .set(RightPanelMode.music),
          onSecondaryTap: (position) => _menu(card, ref, position),
          semanticLabel:
              '${now.playing ? 'Playing' : 'Paused'}: ${now.title}, $detail. '
              'Open the music panel',
          focusRadius: BorderRadius.circular(10),
          builder: (context, state) => AnimatedContainer(
            duration: OcMotion.of(context).hover,
            padding: const EdgeInsets.all(OcSpace.s8),
            decoration: BoxDecoration(
              color: state.active ? colors.hover : null,
              border: Border.all(color: colors.border),
              borderRadius: BorderRadius.circular(10),
            ),
            child: Row(
              children: [
                MusicArtwork(
                  serverKey: channel.server,
                  artworkId: now.artworkId,
                  size: 48,
                ),
                const SizedBox(width: OcSpace.s10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        now.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.bodyStrong.copyWith(color: colors.text),
                      ),
                      Text(
                        detail,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(color: colors.textMuted),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: OcSpace.s6),
                if (now.playing)
                  Icon(
                    OcIcons.graphicEq,
                    size: OcSize.iconRow,
                    color: colors.text,
                  )
                else
                  Text(
                    'Paused',
                    style: OcText.meta.copyWith(color: colors.textMuted),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
