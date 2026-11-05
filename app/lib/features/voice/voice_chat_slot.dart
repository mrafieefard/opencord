import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/attachment_drop.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_float.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// A voice channel's chat in the side slot (§5.15): a float with a 52 header
/// (`Chat · General`, close) over the messages, then the composer float under it.
/// The bottom is level with the user panel.
class VoiceChatSlot extends ConsumerWidget {
  const VoiceChatSlot({
    super.key,
    required this.channel,
    required this.onClose,
  });

  final ChannelRef channel;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final controller = ref.read(chatControllerProvider);
    final channelName = ref.watch(
      serverProvider(
        channel.server,
      ).select((state) => state.data?.channels[channel.channel]?.name ?? ''),
    );

    return AttachmentDropZone(
      channel: channel,
      radius: OcFloats.radius,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            child: OcFloat(
              color: colors.chat,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Container(
                    height: OcFloats.header,
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    decoration: BoxDecoration(
                      border: Border(
                        bottom: BorderSide(color: colors.border, width: 1),
                      ),
                    ),
                    child: Row(
                      children: [
                        Text(
                          'Chat · $channelName',
                          style: OcText.header.copyWith(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            color: colors.text,
                          ),
                        ),
                        const Spacer(),
                        OcIconButton(
                          icon: OcIcons.close,
                          tooltip: 'Close',
                          size: OcIconButtonSize.compact,
                          onPressed: onClose,
                        ),
                      ],
                    ),
                  ),
                  Expanded(
                    child: MessageList(
                      key: ValueKey(('voice-chat-slot', channel)),
                      channel: channel,
                      controller: controller,
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: OcFloats.gap),
          ConstrainedBox(
            constraints: const BoxConstraints(minHeight: OcFloats.bottom),
            child: OcFloat(
              key: ShellKeys.composer,
              color: colors.sidebar,
              child: Composer(
                key: ValueKey(('voice-composer-slot', channel)),
                channel: channel,
                controller: controller,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
