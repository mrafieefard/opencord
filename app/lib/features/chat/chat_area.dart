import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/chat/pinned_bar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

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
    final pins = ref.read(repositoryProvider).capabilities.pins;
    final server = ref.watch(currentServerProvider);
    final channelId = ref.watch(currentChannelProvider);
    final channel = server == null
        ? null
        : ref.watch(
            serverProvider(
              server,
            ).select((state) => state.data?.channels[channelId]),
          );
    final open = server == null || channel == null
        ? null
        : (server: server, channel: channel.id);
    return ColoredBox(
      color: colors.chat,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ChatHeader(
            channel: open,
            membersShown: widget.membersShown,
            onToggleMembers: widget.onToggleMembers,
            onOpenSidebar: widget.onOpenSidebar,
            onJumpTo: _controller.jumpToMessage,
            leadingControls: widget.leadingControls,
            trailingControls: widget.trailingControls,
          ),
          if (open != null && channel!.kind.isTextLike) ...[
            if (pins)
              PinnedBar(
                key: ValueKey(open),
                channel: open,
                onJumpTo: _controller.jumpToMessage,
              ),
            Expanded(
              child: MessageList(
                key: ValueKey(open),
                channel: open,
                controller: _controller,
              ),
            ),
          ] else
            const Expanded(child: SizedBox()),
        ],
      ),
    );
  }
}
