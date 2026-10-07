import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// The main column: the open channel's header and content.
class ChatArea extends ConsumerStatefulWidget {
  const ChatArea({
    super.key,
    required this.membersShown,
    required this.onToggleMembers,
    this.onOpenSidebar,
    this.leadingControls = false,
    this.trailingControls = false,
  });

  /// Window controls (§3.1), when this is the first or the rightmost header.
  final bool leadingControls;
  final bool trailingControls;

  final bool membersShown;
  final VoidCallback onToggleMembers;

  /// Shown as a menu button when the sidebar is collapsed (§3).
  final VoidCallback? onOpenSidebar;

  @override
  ConsumerState<ChatArea> createState() => _ChatAreaState();
}

class _ChatAreaState extends ConsumerState<ChatArea> {
  final _controller = ChatController();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    final channelId = ref.watch(currentChannelProvider);
    final channel = server == null
        ? null
        : ref.watch(
            serverProvider(
              server,
            ).select((state) => state.data?.channels[channelId]),
          );
    return ColoredBox(
      color: colors.chat,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            leadingControls: widget.leadingControls,
            trailingControls: widget.trailingControls,
            child: Row(
              children: [
                if (widget.onOpenSidebar != null)
                  OcIconButton(
                    icon: OcIcons.menu,
                    tooltip: 'Channels',
                    onPressed: widget.onOpenSidebar,
                  ),
                const SizedBox(width: OcSpace.s4),
                Expanded(
                  child: IgnorePointer(
                    child: Text(
                      channel == null
                          ? ''
                          : channel.kind.isTextLike
                          ? '#${channel.name}'
                          : channel.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.header.copyWith(color: colors.text),
                    ),
                  ),
                ),
                OcIconButton(
                  icon: OcIcons.group,
                  tooltip: 'Member list',
                  active: widget.membersShown,
                  onPressed: widget.onToggleMembers,
                ),
              ],
            ),
          ),
          Expanded(
            child: server == null || channel == null
                ? const SizedBox()
                : channel.kind.isTextLike
                ? MessageList(
                    key: ValueKey((server: server, channel: channel.id)),
                    channel: (server: server, channel: channel.id),
                    controller: _controller,
                  )
                : const SizedBox(),
          ),
        ],
      ),
    );
  }
}
