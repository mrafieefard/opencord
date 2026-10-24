import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/hover_bar.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_item.dart';
import 'package:opencord/features/chat/message_line.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/move_card.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/home/private_message_adapter.dart';
import 'package:opencord/features/home/relay_divider.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Message list inside a private conversation (§6.3).
class PrivateMessageList extends ConsumerStatefulWidget {
  const PrivateMessageList({super.key, required this.peer, this.onEditMessage});

  final String peer;
  final void Function(PrivateMessage message)? onEditMessage;

  @override
  ConsumerState<PrivateMessageList> createState() => _PrivateMessageListState();
}

class _PrivateMessageListState extends ConsumerState<PrivateMessageList> {
  final _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(_onScroll);
  }

  @override
  void dispose() {
    _scrollController.removeListener(_onScroll);
    _scrollController.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (_scrollController.hasClients &&
        _scrollController.position.pixels < 150) {
      ref.read(privateConversationProvider(widget.peer).notifier).loadOlder();
    }
  }

  void _showContextMenu(
    BuildContext context,
    Offset position,
    PrivateMessage message,
  ) {
    showOcMenu(
      context: context,
      position: position,
      entries: [
        if (message.state == PrivateSendState.failed) ...[
          OcMenuItem(
            label: 'Retry',
            icon: OcIcons.refresh,
            onSelected: () =>
                ref.read(repositoryProvider).retryPrivateMessage(message.id),
          ),
          OcMenuItem(
            label: 'Delete',
            icon: OcIcons.delete,
            onSelected: () => ref
                .read(repositoryProvider)
                .deletePrivateMessage(widget.peer, message.id),
          ),
        ] else ...[
          OcMenuItem(
            label: 'Copy text',
            icon: OcIcons.contentCopy,
            onSelected: () {
              Clipboard.setData(ClipboardData(text: message.content));
              showOcToast(context, 'Copied');
            },
          ),
          if (message.outgoing && message.kind == PrivateMessageKind.text) ...[
            OcMenuItem(
              label: 'Edit',
              icon: OcIcons.edit,
              onSelected: () => widget.onEditMessage?.call(message),
            ),
            OcMenuItem(
              label: 'Delete',
              icon: OcIcons.delete,
              onSelected: () async {
                final confirmed = await confirmAction(
                  context,
                  title: 'Delete message?',
                  message: 'Are you sure you want to delete this message?',
                  action: 'Delete',
                );
                if (!confirmed || !context.mounted) return;
                try {
                  await ref
                      .read(repositoryProvider)
                      .deletePrivateMessage(widget.peer, message.id);
                } on RepoException catch (e) {
                  if (!context.mounted) return;
                  await showPrivateFailure(
                    context,
                    e,
                    name: ref.read(contactNameProvider(widget.peer)),
                  );
                }
              },
            ),
          ],
        ],
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final state = ref.watch(privateConversationProvider(widget.peer));
    final contact = ref.watch(privateContactsProvider).byPeer[widget.peer];
    final messages = state.messages;
    final compact =
        ref.watch(appSettingsProvider.select((s) => s.density)) ==
        MessageDensity.compact;
    final peerName = ref.watch(contactNameProvider(widget.peer));

    final links = MarkdownContext(
      userName: (_) => null,
      channelName: (_) => null,
      onLink: (url) => openMessageLink(context, ref, url),
    );

    return ListView.builder(
      controller: _scrollController,
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s12,
      ),
      itemCount: messages.length + (state.loading ? 1 : 0),
      itemBuilder: (context, index) {
        if (state.loading && index == 0) {
          return const Center(
            child: Padding(
              padding: EdgeInsets.all(OcSpace.s8),
              child: SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
          );
        }

        final msgIndex = state.loading ? index - 1 : index;
        final m = messages[msgIndex];

        return switch (m.kind) {
          PrivateMessageKind.divider => RelayDivider(
            serverName: m.content,
            serverKey: m.serverKey,
          ),
          PrivateMessageKind.moved => MoveCard(
            peer: widget.peer,
            messageId: m.id,
            serverName: m.content,
            serverKey: m.serverKey,
          ),
          PrivateMessageKind.undecryptable => Padding(
            padding: const EdgeInsets.symmetric(vertical: OcSpace.s8),
            child: Center(
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(OcIcons.lock, size: 14, color: colors.textMuted),
                  const SizedBox(width: OcSpace.s6),
                  Text(
                    'This message couldn\'t be decrypted.',
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                ],
              ),
            ),
          ),
          PrivateMessageKind.text => _buildTextMessage(
            context,
            m,
            msgIndex,
            messages,
            compact,
            peerName,
            contact?.relayServerKey ?? '',
            links,
          ),
        };
      },
    );
  }

  Widget _buildTextMessage(
    BuildContext context,
    PrivateMessage m,
    int index,
    List<PrivateMessage> messages,
    bool compact,
    String peerName,
    String relayServerKey,
    MarkdownContext links,
  ) {
    final chatMsg = asChatMessage(m);
    final isFirst =
        index == 0 ||
        messages[index - 1].outgoing != m.outgoing ||
        messages[index - 1].kind != PrivateMessageKind.text;
    final isLast =
        index == messages.length - 1 ||
        messages[index + 1].outgoing != m.outgoing ||
        messages[index + 1].kind != PrivateMessageKind.text;

    final authorName = m.outgoing
        ? compact
              ? ref.read(localIdentityProvider)?.displayName ?? 'You'
              : 'You'
        : peerName;

    void showMenu(Rect anchor) =>
        _showContextMenu(context, Offset(anchor.right, anchor.bottom), m);
    return Hoverable(
      onSecondaryTap: (pos) => _showContextMenu(context, pos, m),
      semanticLabel: m.content,
      builder: (context, state) => _HoverBarAnchor(
        compact: compact,
        onMore: showMenu,
        builder: (anchor) => compact
            ? anchor(
                CompactMessage(
                  message: chatMsg,
                  serverKey: relayServerKey,
                  first: isFirst,
                  author: authorName,
                  avatar: !isFirst
                      ? null
                      : m.outgoing
                      ? _SelfAvatar(serverKey: relayServerKey, name: authorName)
                      : ContactAvatar(
                          peer: widget.peer,
                          size: OcSize.messageAvatar,
                          presenceDot: false,
                        ),
                  links: links,
                  onRetry: m.state == PrivateSendState.failed
                      ? () => ref
                            .read(repositoryProvider)
                            .retryPrivateMessage(m.id)
                      : null,
                ),
              )
            : MessageLine(
                own: m.outgoing,
                first: isFirst,
                avatar: isLast && !m.outgoing
                    ? ContactAvatar(
                        peer: widget.peer,
                        size: OcSize.messageAvatar,
                        presenceDot: false,
                      )
                    : null,
                bubble: anchor(
                  MessageBubble(
                    message: chatMsg,
                    serverKey: relayServerKey,
                    own: m.outgoing,
                    first: isFirst,
                    last: isLast,
                    author: authorName,
                    links: links,
                    onRetry: m.state == PrivateSendState.failed
                        ? () => ref
                              .read(repositoryProvider)
                              .retryPrivateMessage(m.id)
                        : null,
                  ),
                ),
              ),
      ),
    );
  }
}

/// Your avatar beside your messages in compact density: your member avatar
/// on the relay server, or your initials.
class _SelfAvatar extends ConsumerWidget {
  const _SelfAvatar({required this.serverKey, required this.name});

  final String serverKey;
  final String name;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final self = ref.watch(
      serverProvider(serverKey).select((s) => s.data?.selfMember?.id),
    );
    return self == null
        ? OcAvatar(id: name, name: name, size: OcSize.messageAvatar)
        : MemberAvatar(serverKey: serverKey, userId: self, name: name);
  }
}

/// The hover bar on a compact line's top edge at the right, placed as a
/// channel message's is (`MessageItem`), so it covers no text and can be
/// clicked outside the row. It stays while the pointer is on the row or on
/// the bar. Bubbles get none: their only button would be More, and
/// right-click opens the same menu.
class _HoverBarAnchor extends StatefulWidget {
  const _HoverBarAnchor({
    required this.compact,
    required this.onMore,
    required this.builder,
  });

  final bool compact;
  final ValueChanged<Rect> onMore;

  /// Builds the row, wrapping the bubble (or compact line) in `anchor`.
  final Widget Function(Widget Function(Widget child) anchor) builder;

  @override
  State<_HoverBarAnchor> createState() => _HoverBarAnchorState();
}

class _HoverBarAnchorState extends State<_HoverBarAnchor> {
  final _link = LayerLink();
  final _bar = OverlayPortalController();
  final _hovered = ValueNotifier(false);
  var _overRow = false;
  var _overBar = false;

  @override
  void dispose() {
    _hovered.dispose();
    super.dispose();
  }

  void _show() {
    _hovered.value = true;
    if (widget.compact && !_bar.isShowing) _bar.show();
  }

  void _scheduleHide() {
    scheduleMicrotask(() {
      if (!mounted || _overRow || _overBar) return;
      _hovered.value = false;
      if (_bar.isShowing) _bar.hide();
    });
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) {
        _overRow = true;
        _show();
      },
      onExit: (_) {
        _overRow = false;
        _scheduleHide();
      },
      child: OverlayPortal(
        controller: _bar,
        overlayChildBuilder: (context) => Positioned(
          left: 0,
          top: 0,
          child: CompositedTransformFollower(
            link: _link,
            showWhenUnlinked: false,
            targetAnchor: Alignment.topRight,
            followerAnchor: Alignment.bottomRight,
            offset: const Offset(-OcSpace.s16, HoverActionBar.overlap),
            child: MouseRegion(
              onEnter: (_) {
                _overBar = true;
                _show();
              },
              onExit: (_) {
                _overBar = false;
                _scheduleHide();
              },
              child: HoverActionBar(onMore: widget.onMore),
            ),
          ),
        ),
        child: MessageHover(
          hovered: _hovered,
          child: widget.builder(
            (child) => CompositedTransformTarget(link: _link, child: child),
          ),
        ),
      ),
    );
  }
}
