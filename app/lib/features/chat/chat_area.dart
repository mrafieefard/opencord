import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
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
import 'package:opencord/ui/widgets/measured_height.dart';

/// Whether the open channel is a voice channel showing its chat (Phase 2
/// §12). The chat then takes the member list's place on the right.
final voiceChatShownProvider = Provider<bool>((ref) {
  final server = ref.watch(currentServerProvider);
  final channelId = ref.watch(currentChannelProvider);
  if (server == null || channelId == null) return false;
  final channel = ref.watch(
    serverProvider(server).select((state) => state.data?.channels[channelId]),
  );
  return channel != null &&
      channel.kind == ChannelKind.voice &&
      channel.textInVoice &&
      ref.read(repositoryProvider).capabilities.voice &&
      ref.watch(voiceViewPrefsProvider.select((prefs) => prefs.chatOpen));
});

/// The main column: the open channel's header and content.
class ChatArea extends ConsumerStatefulWidget {
  const ChatArea({
    super.key,
    required this.membersShown,
    required this.onToggleMembers,
    this.musicShown = false,
    this.onSelectMusic,
    this.onOpenSidebar,
    this.leadingControls = false,
    this.trailingControls = false,
  });

  /// Window controls (§3.1), when this is the first or the rightmost header.
  final bool leadingControls;
  final bool trailingControls;

  final bool membersShown;
  final VoidCallback onToggleMembers;
  final bool musicShown;
  final VoidCallback? onSelectMusic;

  /// Shown as a menu button when the sidebar is collapsed (§3).
  final VoidCallback? onOpenSidebar;

  @override
  ConsumerState<ChatArea> createState() => _ChatAreaState();
}

class _ChatAreaState extends ConsumerState<ChatArea> {
  ChatController get _controller => ref.read(chatControllerProvider);

  /// A voice channel's chat beside its tiles (Phase 2 §12), and the room
  /// the tiles and their controls need; narrower, the chat shows alone.
  static const _voiceChatWidth = 380.0;
  static const _stageMinWidth = 600.0;

  /// The composer's height, which the call's control bar beside it
  /// matches.
  final _composerHeight = ValueNotifier<double?>(null);

  @override
  void dispose() {
    _composerHeight.dispose();
    super.dispose();
  }

  /// The channel's messages and composer.
  Widget _channelChat(
    ChannelRef open,
    bool unreachable, {
    required bool pins,
  }) => AttachmentDropZone(
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
          // The last cached state, greyed out while the server cannot be
          // reached (§4.13).
          child: Opacity(
            opacity: unreachable ? 0.55 : 1,
            child: MessageList(
              key: ValueKey(open),
              channel: open,
              controller: _controller,
            ),
          ),
        ),
        MeasuredHeight(
          onChanged: (height) => _composerHeight.value = height,
          child: Composer(
            key: ValueKey(('composer', open)),
            channel: open,
            controller: _controller,
          ),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final voiceChat = ref.watch(voiceChatShownProvider);
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
      child: LayoutBuilder(
        builder: (context, constraints) {
          // Too narrow for the tiles and the chat, the chat shows alone.
          final chatAlone =
              voiceChat &&
              constraints.maxWidth - _voiceChatWidth - 1 < _stageMinWidth;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              ChatHeader(
                voiceChatAlone: chatAlone,
                channel: open,
                memberToggle: !noServers,
                membersShown: widget.membersShown,
                onToggleMembers: widget.onToggleMembers,
                musicShown: widget.musicShown,
                onSelectMusic: widget.onSelectMusic,
                onOpenSidebar: widget.onOpenSidebar,
                onJumpTo: _controller.jumpToMessage,
                leadingControls: widget.leadingControls,
                trailingControls: widget.trailingControls,
              ),
              if (server != null && !noServers)
                ConnectionBanner(serverKey: server),
              if (noServers)
                const Expanded(child: NoServersView())
              else if (open != null && channel!.kind.isTextLike)
                Expanded(child: _channelChat(open, unreachable, pins: pins))
              else if (open != null && channel!.kind == ChannelKind.voice)
                Expanded(
                  child: switch ((voiceChat, chatAlone)) {
                    (false, _) => VoiceView(
                      key: ValueKey(('voice', open)),
                      channel: open,
                    ),
                    (true, true) => _channelChat(
                      open,
                      unreachable,
                      pins: false,
                    ),
                    (true, false) => Row(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Expanded(
                          child: VoiceView(
                            key: ValueKey(('voice', open)),
                            channel: open,
                            composerHeight: _composerHeight,
                          ),
                        ),
                        VerticalDivider(width: 1, color: colors.border),
                        SizedBox(
                          width: _voiceChatWidth,
                          child: _channelChat(open, unreachable, pins: false),
                        ),
                      ],
                    ),
                  },
                )
              else
                const Expanded(child: SizedBox()),
            ],
          );
        },
      ),
    );
  }
}
