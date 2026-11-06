import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/features/chat/audio_player_bar.dart';
import 'package:opencord/features/chat/attachment_drop.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/connection_banner.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/chat/pinned_bar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/no_servers.dart';
import 'package:opencord/features/shell/side_slot.dart';
import 'package:opencord/features/voice/voice_view.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

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
    this.onOpenSidebar,
    this.includeComposer = true,
    this.voiceChatAlone,
  });

  final bool includeComposer;
  final VoidCallback? onOpenSidebar;
  final bool? voiceChatAlone;

  @override
  ConsumerState<ChatArea> createState() => _ChatAreaState();
}

class _ChatAreaState extends ConsumerState<ChatArea> {
  ChatController get _controller => ref.read(chatControllerProvider);

  /// The channel's messages and composer.
  Widget _channelChat(
    ChannelRef open,
    bool unreachable, {
    required bool pins,
    bool? includeComposer,
    bool fillsMain = true,
  }) {
    final composer = includeComposer ?? widget.includeComposer;
    final hasPins = pins && ref.watch(pinsProvider(open)).isNotEmpty;
    final list = MessageList(
      key: ValueKey(open),
      channel: open,
      controller: _controller,
      topInset: hasPins ? 68.0 : 0.0,
    );
    final messages = fillsMain ? MainInnerWidth(child: list) : list;
    final body = Stack(
      children: [
        Positioned.fill(
          child: Opacity(opacity: unreachable ? 0.55 : 1, child: messages),
        ),
        if (hasPins)
          Positioned(
            top: 8,
            left: 12,
            right: 12,
            child: PinnedBar(
              key: ValueKey(('pins', open)),
              channel: open,
              onJumpTo: _controller.jumpToMessage,
            ),
          ),
      ],
    );
    final chat = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(child: body),
        if (composer)
          Composer(
            key: ValueKey(('composer', open)),
            channel: open,
            controller: _controller,
          ),
      ],
    );
    // Without its composer the shell's zone takes the drop, around both floats.
    return composer ? AttachmentDropZone(channel: open, child: chat) : chat;
  }

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
          // Too narrow for the tiles and the chat at its default width (Phase
          // 2 §12), the chat shows alone. This goes by the width main ends at,
          // so a pane on its way doesn't swap them back and forth.
          final width =
              ShellMetrics.maybeOf(context)?.mainInner ?? constraints.maxWidth;
          final chatAlone =
              widget.voiceChatAlone ??
              (voiceChat &&
                  width - OcFloats.voiceChat - 1 < OcFloats.voiceStageMin);
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              ChatHeader(
                voiceChatAlone: chatAlone,
                channel: open,
                onOpenSidebar: widget.onOpenSidebar,
                onJumpTo: _controller.jumpToMessage,
              ),
              if (server != null && !noServers)
                ConnectionBanner(serverKey: server),
              const AudioPlayerBar(),
              if (noServers)
                const Expanded(child: NoServersView())
              else if (open != null && channel!.kind.isTextLike)
                Expanded(child: _channelChat(open, unreachable, pins: pins))
              else if (open != null && channel!.kind == ChannelKind.voice)
                Expanded(
                  child: (voiceChat && chatAlone)
                      ? _channelChat(
                          open,
                          unreachable,
                          pins: false,
                          includeComposer: widget.includeComposer,
                        )
                      : VoiceView(
                          key: ValueKey(('voice', open)),
                          channel: open,
                        ),
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
