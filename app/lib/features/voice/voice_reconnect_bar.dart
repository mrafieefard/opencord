import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/providers/voice_reconnect.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_float.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// A float under the title row while this device is out of a call it didn't
/// leave, as Discord's (D88): why, and a button to get back in.
///
/// 40 px tall, margins 0 8 8, radius 16, elevated background, float shadow.
/// Appears and leaves by height on pane / paneOut (§5.19).
class VoiceReconnectBar extends ConsumerStatefulWidget {
  const VoiceReconnectBar({super.key});

  static const barKey = Key('voice-reconnect-bar');
  static const height = 40.0;

  @override
  ConsumerState<VoiceReconnectBar> createState() => _VoiceReconnectBarState();
}

class _VoiceReconnectBarState extends ConsumerState<VoiceReconnectBar>
    with SingleTickerProviderStateMixin {
  bool _busy = false;
  late final AnimationController _controller;
  VoiceDrop? _lastDrop;
  String? _lastChannel;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController.unbounded(vsync: this, value: 0.0);
    _controller.addListener(() {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _reconnect() async {
    setState(() => _busy = true);
    try {
      await ref.read(voiceReconnectProvider.notifier).reconnect();
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _syncAnimation(bool shown) {
    final disableAnimations = MediaQuery.disableAnimationsOf(context);
    if (disableAnimations) {
      _controller.value = shown ? 1.0 : 0.0;
      return;
    }
    final target = shown ? 1.0 : 0.0;
    if ((_controller.value - target).abs() < 0.001) return;
    if (shown) {
      _controller.animateWith(
        OcSpring.pane.simulation(_controller.value, 1.0, _controller.velocity),
      );
    } else {
      _controller.animateWith(
        OcSpring.paneOut.simulation(
          _controller.value,
          0.0,
          _controller.velocity,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final drop = ref.watch(voiceReconnectProvider);
    final channel = drop == null
        ? null
        : ref.watch(
            serverProvider(
              drop.serverKey,
            ).select((server) => server.data?.channels[drop.channelId]?.name),
          );

    final shown = drop != null && channel != null;
    if (shown) {
      _lastDrop = drop;
      _lastChannel = channel;
    }

    _syncAnimation(shown);

    final progress = _controller.value.clamp(0.0, 1.0);
    if (progress <= 0.001 && !shown) return const SizedBox.shrink();

    final activeDrop = drop ?? _lastDrop;
    final activeChannel = channel ?? _lastChannel;
    if (activeDrop == null || activeChannel == null) {
      return const SizedBox.shrink();
    }

    final colors = context.oc;
    final text = switch (activeDrop.reason) {
      VoiceDropReason.closed =>
        'Opencord closed while you were in $activeChannel.',
      VoiceDropReason.elsewhere =>
        'Your call in $activeChannel moved to another device.',
      VoiceDropReason.refused => switch (activeDrop.message) {
        final message? when message.isNotEmpty =>
          "Couldn't get back into $activeChannel: $message",
        _ => "Couldn't get back into $activeChannel.",
      },
    };

    return ClipRect(
      child: Align(
        alignment: Alignment.topCenter,
        heightFactor: progress,
        child: Padding(
          padding: const EdgeInsets.only(left: 8, right: 8, bottom: 8),
          child: SizedBox(
            height: VoiceReconnectBar.height,
            child: OcFloat(
              key: VoiceReconnectBar.barKey,
              color: colors.elevated,
              radius: 16,
              child: Container(
                padding: const EdgeInsets.only(
                  left: OcSpace.s16,
                  right: OcSpace.s8,
                ),
                child: Row(
                  children: [
                    Icon(
                      OcIcons.headsetOff,
                      size: OcSize.iconInline,
                      color: colors.textSecondary,
                    ),
                    const SizedBox(width: OcSpace.s8),
                    Expanded(
                      child: Text(
                        text,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(color: colors.text),
                      ),
                    ),
                    const SizedBox(width: OcSpace.s12),
                    OcButton.primary(
                      label: 'Reconnect',
                      dense: true,
                      busy: _busy,
                      onPressed: _busy ? null : _reconnect,
                    ),
                    const SizedBox(width: OcSpace.s4),
                    OcIconButton(
                      icon: OcIcons.close,
                      tooltip: 'Dismiss',
                      size: OcIconButtonSize.compact,
                      onPressed: ref
                          .read(voiceReconnectProvider.notifier)
                          .dismiss,
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
