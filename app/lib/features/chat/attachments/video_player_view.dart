import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

class VideoPlayerView extends ConsumerStatefulWidget {
  const VideoPlayerView({
    super.key,
    required this.serverKey,
    required this.attachment,
  });

  final String serverKey;
  final Attachment attachment;

  @override
  ConsumerState<VideoPlayerView> createState() => _VideoPlayerViewState();
}

class _VideoPlayerViewState extends ConsumerState<VideoPlayerView> {
  MediaHandle? _handle;
  bool _loading = true;
  String? _error;
  bool _unsupported = false;
  double _volume = 1.0;
  bool _muted = false;
  bool _draggingSeek = false;
  double _dragPositionMs = 0;
  PlaybackNotifier? _playbackNotifier;

  AttachmentPlayback get _item =>
      AttachmentPlayback(widget.serverKey, widget.attachment.id);

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _initPlayer();
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _playbackNotifier = ref.read(playbackProvider.notifier);
  }

  @override
  void didUpdateWidget(VideoPlayerView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.attachment.id != widget.attachment.id ||
        oldWidget.serverKey != widget.serverKey) {
      _playbackNotifier?.mediaClose();
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _initPlayer();
      });
    }
  }

  @override
  void dispose() {
    _playbackNotifier?.mediaClose();
    super.dispose();
  }

  void _initPlayer() {
    final supported = ref.read(repositoryProvider).mediaSupported();
    if (!supported) {
      setState(() {
        _unsupported = true;
        _loading = false;
      });
      return;
    }

    setState(() {
      _loading = true;
      _error = null;
      _unsupported = false;
    });

    _openMedia();
  }

  Future<void> _openMedia() async {
    try {
      final handle = await ref.read(playbackProvider.notifier).mediaOpen(_item);
      if (!mounted) return;
      setState(() {
        _handle = handle;
        _loading = false;
      });
      ref.read(playbackProvider.notifier).mediaPlay(_item);
    } on RepoException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.message;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.toString();
        _loading = false;
      });
    }
  }

  void _togglePlay() {
    final playback = ref.read(playbackProvider);
    final notifier = ref.read(playbackProvider.notifier);
    if (playback.playing) {
      notifier.pause(_item);
    } else {
      notifier.resume(_item);
    }
  }

  void _toggleMute() {
    setState(() {
      _muted = !_muted;
    });
    ref.read(playbackProvider.notifier).mediaSetVolume(_volume, _muted);
  }

  void _onVolumeChanged(double value) {
    setState(() {
      _volume = value;
      _muted = value == 0;
    });
    ref.read(playbackProvider.notifier).mediaSetVolume(_volume, _muted);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final playback = ref.watch(playbackProvider);

    if (_unsupported) {
      return Container(
        width: 480,
        height: 320,
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
          border: Border.all(color: colors.border),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(OcIcons.movie, size: 48, color: colors.textSecondary),
            const SizedBox(height: OcSpace.s12),
            Text(
              "This video can't play on this computer yet",
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
            const SizedBox(height: OcSpace.s16),
            OcButton.ghost(
              label: 'Download',
              dense: true,
              onPressed: () => saveAttachment(
                context,
                ref,
                widget.serverKey,
                widget.attachment,
              ),
            ),
          ],
        ),
      );
    }

    if (_loading) {
      return const Center(
        child: SizedBox(
          width: 48,
          height: 48,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
      );
    }

    if (_error != null || playback.error != null) {
      return Container(
        padding: const EdgeInsets.all(OcSpace.s16),
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              OcIcons.error,
              size: OcSize.iconButton,
              color: colors.textSecondary,
            ),
            const SizedBox(height: OcSpace.s8),
            Text(
              _error ?? playback.error ?? 'Playback failed',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
            const SizedBox(height: OcSpace.s8),
            OcButton.ghost(label: 'Retry', dense: true, onPressed: _initPlayer),
          ],
        ),
      );
    }

    final durationMs = _handle?.durationMs ?? widget.attachment.durationMs ?? 0;
    final currentPosMs = _draggingSeek
        ? _dragPositionMs.toInt()
        : playback.positionMs.clamp(0, durationMs);

    final textureId = _handle?.textureId;
    final hasVideo = _handle?.hasVideo ?? false;
    final videoWidth = _handle?.width ?? 320;
    final videoHeight = _handle?.height ?? 240;
    final aspectRatio = (videoWidth > 0 && videoHeight > 0)
        ? videoWidth / videoHeight
        : 16 / 9;

    return CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.space): _togglePlay},
      child: Focus(
        autofocus: true,
        child: Stack(
          alignment: Alignment.center,
          children: [
            // Video display or audio surface
            if (hasVideo && textureId != null)
              Center(
                child: AspectRatio(
                  aspectRatio: aspectRatio,
                  child: Texture(textureId: textureId),
                ),
              )
            else
              Center(
                child: Container(
                  width: 320,
                  height: 240,
                  color: colors.surface,
                  child: Center(
                    child: Icon(
                      hasVideo ? OcIcons.movie : OcIcons.musicNote,
                      size: 64,
                      color: colors.textSecondary,
                    ),
                  ),
                ),
              ),

            // Bottom control bar
            Positioned(
              left: OcSpace.s16,
              right: OcSpace.s16,
              bottom: OcSpace.s16,
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s12,
                  vertical: OcSpace.s8,
                ),
                decoration: BoxDecoration(
                  color: colors.scrim.withValues(alpha: 0.85),
                  borderRadius: BorderRadius.circular(OcRadius.bubble),
                  border: Border.all(
                    color: colors.border.withValues(alpha: 0.3),
                  ),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    // Seek slider
                    SliderTheme(
                      data: SliderTheme.of(context).copyWith(
                        trackHeight: 3,
                        thumbShape: const RoundSliderThumbShape(
                          enabledThumbRadius: 6,
                        ),
                        activeTrackColor: colors.text,
                        inactiveTrackColor: colors.textSecondary.withValues(
                          alpha: 0.4,
                        ),
                        thumbColor: colors.text,
                      ),
                      child: Slider(
                        value: durationMs > 0
                            ? currentPosMs.toDouble().clamp(
                                0.0,
                                durationMs.toDouble(),
                              )
                            : 0.0,
                        max: durationMs > 0 ? durationMs.toDouble() : 1.0,
                        onChangeStart: (v) {
                          setState(() {
                            _draggingSeek = true;
                            _dragPositionMs = v;
                          });
                        },
                        onChanged: (v) {
                          setState(() {
                            _dragPositionMs = v;
                          });
                        },
                        onChangeEnd: (v) {
                          setState(() {
                            _draggingSeek = false;
                          });
                          ref
                              .read(playbackProvider.notifier)
                              .seek(_item, v.toInt());
                        },
                      ),
                    ),

                    Row(
                      children: [
                        // Play/Pause
                        OcIconButton(
                          icon: playback.playing
                              ? OcIcons.pause
                              : OcIcons.playArrow,
                          tooltip: playback.playing ? 'Pause' : 'Play',
                          onPressed: _togglePlay,
                        ),
                        const SizedBox(width: OcSpace.s8),

                        // m:ss / m:ss
                        Text(
                          '${formatDurationMs(currentPosMs)} / ${formatDurationMs(durationMs)}',
                          style: OcText.small.copyWith(color: colors.text),
                        ),

                        const Spacer(),

                        // Mute toggle
                        OcIconButton(
                          icon: _muted || _volume == 0
                              ? OcIcons.volumeOff
                              : OcIcons.volumeUp,
                          size: OcIconButtonSize.compact,
                          tooltip: _muted ? 'Unmute' : 'Mute',
                          onPressed: _toggleMute,
                        ),

                        // Volume slider
                        SizedBox(
                          width: 80,
                          child: SliderTheme(
                            data: SliderTheme.of(context).copyWith(
                              trackHeight: 3,
                              thumbShape: const RoundSliderThumbShape(
                                enabledThumbRadius: 5,
                              ),
                              activeTrackColor: colors.text,
                              inactiveTrackColor: colors.textSecondary
                                  .withValues(alpha: 0.4),
                              thumbColor: colors.text,
                            ),
                            child: Slider(
                              value: _muted ? 0.0 : _volume,
                              onChanged: _onVolumeChanged,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
