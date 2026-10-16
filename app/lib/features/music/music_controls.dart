import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// How often the bar moves on by itself while music plays (§0.1).
const progressTick = Duration(milliseconds: 250);

/// The track's progress (§19.4): a 4 px bar with the time elapsed and the
/// total. It moves 4 times a second while playing and nothing rebuilds but
/// it; dragging seeks on release. Streams show LIVE.
class MusicProgress extends ConsumerStatefulWidget {
  const MusicProgress({super.key, required this.channel});

  final MusicChannel channel;

  @override
  ConsumerState<MusicProgress> createState() => _MusicProgressState();
}

class _MusicProgressState extends ConsumerState<MusicProgress> {
  Timer? _tick;

  /// Where the thumb is while dragged.
  double? _dragging;

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  void _ticking(bool playing) {
    if (playing && _tick == null) {
      _tick = Timer.periodic(progressTick, (_) {
        if (mounted && TickerMode.valuesOf(context).enabled) setState(() {});
      });
    } else if (!playing) {
      _tick?.cancel();
      _tick = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final session = ref.watch(
      musicSessionProvider(widget.channel).select((value) => value.value),
    );
    final access = ref.watch(musicAccessProvider(widget.channel));
    final item = session?.nowPlaying;
    _ticking(session?.playing ?? false);
    final track = item?.track;
    final length = track?.duration ?? Duration.zero;
    final at = session?.positionAt(DateTime.now()) ?? Duration.zero;
    final shown = _dragging == null
        ? at
        : Duration(milliseconds: (_dragging! * length.inMilliseconds).round());
    final seekable =
        track != null &&
        track.isSeekable &&
        !track.isStream &&
        length > Duration.zero;
    final why = track == null
        ? null
        : track.isStream
        ? 'Live streams can’t be seeked'
        : !track.isSeekable
        ? 'This track can’t be seeked'
        : access.why(Permissions.musicControl);
    final style = OcText.meta.copyWith(
      color: colors.textMuted,
      fontFeatures: const [FontFeature.tabularFigures()],
    );
    final fraction = length > Duration.zero
        ? (shown.inMilliseconds / length.inMilliseconds).clamp(0.0, 1.0)
        : 0.0;
    final slider = SliderTheme(
      data: SliderTheme.of(context).copyWith(
        trackHeight: 4,
        thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
        overlayShape: const RoundSliderOverlayShape(overlayRadius: 12),
        activeTrackColor: colors.text,
        inactiveTrackColor: colors.border,
        thumbColor: colors.text,
        disabledActiveTrackColor: colors.textSecondary,
        disabledInactiveTrackColor: colors.border,
        disabledThumbColor: Colors.transparent,
      ),
      child: Slider(
        value: track?.isStream ?? false ? 1 : fraction,
        semanticFormatterCallback: (_) =>
            '${formatTrackTime(shown)} of ${formatTrackTime(length)}',
        onChanged: seekable && why == null
            ? (value) => setState(() => _dragging = value)
            : null,
        onChangeEnd: seekable && why == null
            ? (value) {
                setState(() => _dragging = null);
                final position = Duration(
                  milliseconds: (value * length.inMilliseconds).round(),
                );
                final expected = item?.id;
                musicAction(
                  context,
                  ref,
                  widget.channel,
                  (repository) => repository.musicSeek(
                    widget.channel.server,
                    widget.channel.channel,
                    position,
                    expectedItemId: expected,
                  ),
                );
              }
            : null,
      ),
    );
    return RepaintBoundary(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            height: 24,
            child: why == null ? slider : Tooltip(message: why, child: slider),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
            child: Row(
              children: [
                Text(formatTrackTime(shown), style: style),
                const Spacer(),
                Text(
                  track == null
                      ? '0:00'
                      : track.isStream
                      ? 'LIVE'
                      : formatTrackTime(length),
                  style: style,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// Shuffle · Previous · Play/Pause · Next · Repeat (§19.4). Without
/// Control playback, Next skips one's own track, or is a vote when vote
/// skip is on (L7).
class MusicTransport extends ConsumerWidget {
  const MusicTransport({super.key, required this.channel});

  final MusicChannel channel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(
      musicSessionProvider(channel).select((value) => value.value),
    );
    final access = ref.watch(musicAccessProvider(channel));
    final item = session?.nowPlaying;
    final control = access.canControl;
    final why = access.why(Permissions.musicControl);
    final (server, channelId) = (channel.server, channel.channel);
    void act(
      Future<MusicSession?> Function(OpencordRepository repository) run,
    ) => musicAction(context, ref, channel, run);
    final repeat = session?.repeat ?? MusicRepeat.off;
    final playing =
        session?.state == MusicPlayState.playing ||
        session?.state == MusicPlayState.loading;
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        _ModeButton(
          icon: OcIcons.shuffle,
          tooltip:
              why ??
              (session?.shuffle ?? false ? 'Shuffle: on' : 'Shuffle: off'),
          active: session?.shuffle ?? false,
          onPressed: control && session != null
              ? () => act(
                  (repository) => repository.musicSetMode(
                    server,
                    channelId,
                    shuffle: !session.shuffle,
                  ),
                )
              : null,
        ),
        const SizedBox(width: OcSpace.s8),
        _TransportButton(
          icon: OcIcons.skipPrevious,
          tooltip: why ?? 'Previous',
          onPressed: control && item != null
              ? () => act(
                  (repository) => repository.musicSkip(
                    server,
                    channelId,
                    forward: false,
                    expectedItemId: item.id,
                  ),
                )
              : null,
        ),
        const SizedBox(width: OcSpace.s12),
        _PlayButton(
          playing: playing,
          tooltip: why ?? (playing ? 'Pause' : 'Play'),
          onPressed: control && item != null
              ? () => act(
                  (repository) =>
                      repository.musicPause(server, channelId, paused: playing),
                )
              : null,
        ),
        const SizedBox(width: OcSpace.s12),
        _TransportButton(
          icon: OcIcons.skipNext,
          tooltip: access.canSkip(item?.requesterId) ? 'Next' : why ?? 'Next',
          onPressed: item != null && access.canSkip(item.requesterId)
              ? () => act(
                  (repository) => repository.musicSkip(
                    server,
                    channelId,
                    forward: true,
                    expectedItemId: item.id,
                  ),
                )
              : null,
        ),
        const SizedBox(width: OcSpace.s8),
        _ModeButton(
          icon: repeat == MusicRepeat.track
              ? OcIcons.repeatOne
              : OcIcons.repeat,
          tooltip:
              why ??
              switch (repeat) {
                MusicRepeat.off => 'Repeat: off',
                MusicRepeat.queue => 'Repeat: queue',
                MusicRepeat.track => 'Repeat: track',
              },
          active: repeat != MusicRepeat.off,
          onPressed: control && session != null
              ? () => act(
                  (repository) => repository.musicSetMode(
                    server,
                    channelId,
                    repeat: repeat.next,
                  ),
                )
              : null,
        ),
      ],
    );
  }
}

/// 44 px, inverted (§19.4).
class _PlayButton extends StatelessWidget {
  const _PlayButton({
    required this.playing,
    required this.tooltip,
    required this.onPressed,
  });

  final bool playing;
  final String tooltip;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final enabled = onPressed != null;
    return Tooltip(
      message: tooltip,
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: onPressed,
        semanticLabel: playing ? 'Pause' : 'Play',
        focusRadius: BorderRadius.circular(22),
        builder: (context, state) => AnimatedContainer(
          duration: OcMotion.of(context).hover,
          width: 44,
          height: 44,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: enabled
                ? state.active
                      ? colors.textSecondary
                      : colors.accent
                : colors.hover,
          ),
          child: Icon(
            playing ? OcIcons.pause : OcIcons.playArrow,
            size: 26,
            color: enabled ? colors.onAccent : colors.textMuted,
          ),
        ),
      ),
    );
  }
}

class _TransportButton extends StatelessWidget {
  const _TransportButton({
    required this.icon,
    required this.tooltip,
    required this.onPressed,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) => _RoundIcon(
    icon: icon,
    tooltip: tooltip,
    onPressed: onPressed,
    iconSize: 26,
  );
}

/// Shuffle and repeat: active in the text colour with a 4 px dot below.
class _ModeButton extends StatelessWidget {
  const _ModeButton({
    required this.icon,
    required this.tooltip,
    required this.active,
    required this.onPressed,
  });

  final IconData icon;
  final String tooltip;
  final bool active;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) => _RoundIcon(
    icon: icon,
    tooltip: tooltip,
    onPressed: onPressed,
    iconSize: 20,
    active: active,
  );
}

class _RoundIcon extends StatelessWidget {
  const _RoundIcon({
    required this.icon,
    required this.tooltip,
    required this.onPressed,
    required this.iconSize,
    this.active,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback? onPressed;
  final double iconSize;

  /// Null for buttons without a mode.
  final bool? active;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final enabled = onPressed != null;
    final color = !enabled
        ? colors.textMuted
        : active == false
        ? colors.textMuted
        : colors.text;
    return Tooltip(
      message: tooltip,
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: onPressed,
        semanticLabel: tooltip,
        selected: active ?? false,
        focusRadius: BorderRadius.circular(18),
        builder: (context, state) => AnimatedContainer(
          duration: OcMotion.of(context).hover,
          width: 36,
          height: 36,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: enabled && state.active ? colors.hover : null,
          ),
          child: Stack(
            alignment: Alignment.center,
            children: [
              Icon(icon, size: iconSize, color: color),
              if (active ?? false)
                Positioned(
                  bottom: 3,
                  child: Container(
                    width: 4,
                    height: 4,
                    decoration: BoxDecoration(
                      color: colors.text,
                      shape: BoxShape.circle,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// This person's music volume and mute (§19.8): a slider from 0 to
/// 200 %, the same setting everywhere.
class MusicVolumeControls extends ConsumerWidget {
  const MusicVolumeControls({super.key});

  static const step = 5;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final listening = ref.watch(musicListeningProvider);
    final notifier = ref.read(musicListeningProvider.notifier);
    final shown = '${listening.volume}%';
    return Material(
      type: MaterialType.transparency,
      child: SizedBox(
        width: 240,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 6, 10, 0),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      'Your music volume',
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                  ),
                  Text(
                    shown,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
              ),
            ),
            Slider(
              value: listening.volume.toDouble(),
              max: 200,
              divisions: 200 ~/ step,
              label: shown,
              semanticFormatterCallback: (_) => shown,
              onChanged: (raw) =>
                  notifier.update((l) => l.copyWith(volume: raw.round())),
            ),
            OcMenuToggle(
              label: 'Mute music',
              checked: listening.muted,
              onChanged: (muted) =>
                  notifier.update((l) => l.copyWith(muted: muted)),
            ),
          ],
        ),
      ),
    );
  }
}

/// The speaker button that opens [MusicVolumeControls].
class MusicVolumeButton extends ConsumerWidget {
  const MusicVolumeButton({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final listening = ref.watch(musicListeningProvider);
    final off = listening.muted || listening.volume == 0;
    return Builder(
      builder: (button) => _RoundIcon(
        icon: off ? OcIcons.volumeOff : OcIcons.volumeUp,
        tooltip: off ? 'Your music volume: muted' : 'Your music volume',
        iconSize: 20,
        onPressed: () => showPopover<void>(
          context: button,
          anchor: globalRectOf(button),
          side: PopoverSide.above,
          builder: (_) => const MusicVolumeControls(),
        ),
      ),
    );
  }
}
