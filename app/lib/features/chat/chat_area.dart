import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachment_drop.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/connection_banner.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/chat/pinned_bar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/no_servers.dart';
import 'package:opencord/features/voice/voice_view.dart';
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
  ChatController get _controller => ref.read(chatControllerProvider);

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
    final noServers = ref.watch(
      serverListProvider.select((servers) => servers.isEmpty),
    );
    final unreachable =
        server != null &&
        ref.watch(
          serverProvider(
            server,
          ).select((state) => serverUnreachable(state.connection)),
        );
    return ColoredBox(
      color: colors.chat,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ChatHeader(
            channel: open,
            memberToggle: !noServers,
            membersShown: widget.membersShown,
            onToggleMembers: widget.onToggleMembers,
            onOpenSidebar: widget.onOpenSidebar,
            onJumpTo: _controller.jumpToMessage,
            leadingControls: widget.leadingControls,
            trailingControls: widget.trailingControls,
          ),
          if (server != null && !noServers) ConnectionBanner(serverKey: server),
          if (noServers)
            const Expanded(child: NoServersView())
          else if (open != null && channel!.kind.isTextLike)
            Expanded(
              child: AttachmentDropZone(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (pins)
                      PinnedBar(
                        key: ValueKey(('pins', open)),
                        channel: open,
                        onJumpTo: _controller.jumpToMessage,
                      ),
                    Expanded(
                      // The last cached state, greyed out while the
                      // server cannot be reached (§4.13).
                      child: Opacity(
                        opacity: unreachable ? 0.55 : 1,
                        child: MessageList(
                          key: ValueKey(open),
                          channel: open,
                          controller: _controller,
                        ),
                      ),
                    ),
                    Composer(
                      key: ValueKey(('composer', open)),
                      channel: open,
                      controller: _controller,
                    ),
                  ],
                ),
              ),
            )
          else if (open != null && channel!.kind == ChannelKind.voice)
            Expanded(
              child: VoiceView(key: ValueKey(('voice', open)), channel: open),
            )
          else
            const Expanded(child: SizedBox()),
        ],
      ),
    );
  }
}
