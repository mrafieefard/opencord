import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/providers/voice_reconnect.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Across the top of the app while this device is out of a call it didn't
/// leave, as Discord's (D88): why, and a button to get back in. Nothing
/// shows for a channel the server no longer has.
class VoiceReconnectBar extends ConsumerStatefulWidget {
  const VoiceReconnectBar({super.key});

  static const barKey = Key('voice-reconnect-bar');
  static const height = 40.0;

  @override
  ConsumerState<VoiceReconnectBar> createState() => _VoiceReconnectBarState();
}

class _VoiceReconnectBarState extends ConsumerState<VoiceReconnectBar> {
  bool _busy = false;

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
    if (drop == null || channel == null) return const SizedBox.shrink();
    final colors = context.oc;
    final text = switch (drop.reason) {
      VoiceDropReason.closed => 'Opencord closed while you were in $channel.',
      VoiceDropReason.elsewhere =>
        'Your call in $channel moved to another device.',
      VoiceDropReason.refused => switch (drop.message) {
        final message? when message.isNotEmpty =>
          "Couldn't get back into $channel: $message",
        _ => "Couldn't get back into $channel.",
      },
    };
    return Container(
      key: VoiceReconnectBar.barKey,
      height: VoiceReconnectBar.height,
      padding: const EdgeInsets.only(left: OcSpace.s16, right: OcSpace.s8),
      decoration: BoxDecoration(
        color: colors.elevated,
        border: Border(bottom: BorderSide(color: colors.border)),
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
            onPressed: ref.read(voiceReconnectProvider.notifier).dismiss,
          ),
        ],
      ),
    );
  }
}
