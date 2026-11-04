import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/chat_scroll.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/hover_bar.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_item.dart';
import 'package:opencord/features/chat/message_line.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/move_card.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/home/private_message_adapter.dart';
import 'package:opencord/features/home/relay_divider.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
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
  static const _newerKey = ValueKey('newer');

  /// How many older pages may load to find where the reader left off.
  static const _pagesToRestore = 10;

  late final ScrollMemory _memory;
  ChatScrollController? _scroll;

  /// Where the newer half starts: at [_splitId], or after [_splitAfterId]
  /// (the newest message when the chat opened at the end); neither, at the
  /// top. New messages join the newer half, so the view stays still.
  String? _splitId;
  String? _splitAfterId;
  int _split = 0;

  int _pagesLoaded = 0;
  PrivateScroll? _restoring;
  PrivateScroll? _saved;

  /// Whether [_saved] says where the reader is: false until the chat has
  /// opened and been looked at, so leaving before then forgets nothing.
  bool _tracking = false;
  bool _checksScheduled = false;
  final _onScreen = <String, _TrackedMessageState>{};

  PrivateConversationNotifier get _conversation =>
      ref.read(privateConversationProvider(widget.peer).notifier);

  @override
  void initState() {
    super.initState();
    _memory = ref.read(scrollMemoryProvider);
  }

  @override
  void dispose() {
    if (_tracking) _memory.savePrivate(widget.peer, _saved);
    _scroll?.dispose();
    super.dispose();
  }

  /// Where the chat opens (§6): where the reader left it, loading older
  /// pages to reach it, else at the newest message. Null while a page
  /// loads.
  ChatScrollController? _open(PrivateConversationState state) {
    final messages = state.messages;
    final saved = _memory.readPrivate(widget.peer);
    if (saved != null) {
      final at = messages.indexWhere((m) => m.id == saved.messageId);
      // A page past it, if there is one, so it is grouped as it was.
      if (at > 0 || (at == 0 && !state.hasMore)) {
        _splitId = saved.messageId;
        _restoring = saved;
        return ChatScrollController(start: StartAtAnchor(saved.fromTop))
          ..addListener(_scheduleChecks);
      }
      if (state.hasMore && _pagesLoaded < _pagesToRestore) {
        _pagesLoaded++;
        // Not during this build.
        scheduleMicrotask(_conversation.loadOlder);
        return null;
      }
    }
    _splitAfterId = messages.lastOrNull?.id;
    return ChatScrollController()..addListener(_scheduleChecks);
  }

  int _splitOf(List<PrivateMessage> messages) {
    int at(String? id) =>
        id == null ? -1 : messages.indexWhere((m) => m.id == id);
    final first = at(_splitId);
    final after = at(_splitAfterId);
    if (first != -1) {
      _split = first;
    } else if (after != -1) {
      _split = after + 1;
    } else if (_splitId == null && _splitAfterId == null) {
      _split = 0;
    }
    return _split = _split.clamp(0, messages.length);
  }

  /// Sending goes to the newest message.
  void _onMessages(
    PrivateConversationState? previous,
    PrivateConversationState next,
  ) {
    final last = next.messages.lastOrNull;
    if (previous == null || last == null || !last.outgoing) return;
    if (last.id == previous.messages.lastOrNull?.id) return;
    if (previous.messages.any((m) => m.id == last.id)) return;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (mounted) _scroll?.scrollToEnd(duration: OcMotion.of(context).morph);
    });
  }

  // Watching the scroll ----------------------------------------------------

  /// Runs [_checks] after this frame: scroll listeners can fire during
  /// layout, when nothing may be rebuilt.
  void _scheduleChecks() {
    if (_checksScheduled) return;
    _checksScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) => _checks());
    SchedulerBinding.instance.ensureVisualUpdate();
  }

  void _checks() {
    _checksScheduled = false;
    final scroll = _scroll;
    if (!mounted || scroll == null || !scroll.hasClients) return;
    final position = scroll.position;
    if (!position.hasContentDimensions) return;
    if (_restoring case final saved?) {
      _restoring = null;
      final top = _topOf(saved.messageId);
      if (top != null) position.jumpTo(position.pixels + top - saved.fromTop);
    }
    final state = ref.read(privateConversationProvider(widget.peer));
    if (state.hasMore &&
        !state.loading &&
        position.extentBefore < position.viewportDimension) {
      _conversation.loadOlder();
    }
    _saved = position.atEnd ? null : _topPosition();
    _tracking = true;
    _memory.savePrivate(widget.peer, _saved);
  }

  RenderBox? get _box {
    final box = context.findRenderObject();
    return box is RenderBox && box.hasSize ? box : null;
  }

  double? _topOf(String messageId) {
    final box = _box;
    if (box == null) return null;
    return _onScreen[messageId]?.rectIn(box)?.top;
  }

  /// The topmost sent message in view, and how far below the top it is.
  PrivateScroll? _topPosition() {
    final box = _box;
    if (box == null) return null;
    PrivateScroll? top;
    for (final MapEntry(key: id, value: tracked) in _onScreen.entries) {
      final rect = tracked.rectIn(box);
      if (rect == null || rect.bottom <= 0 || rect.top >= box.size.height) {
        continue;
      }
      if (!tracked.widget.sent) continue;
      if (top == null || rect.top < top.fromTop) {
        top = PrivateScroll(messageId: id, fromTop: rect.top);
      }
    }
    return top;
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

    ref.listen(privateConversationProvider(widget.peer), _onMessages);
    if (_scroll == null && state.loading) return const _Spinner();
    final scroll = _scroll ??= _open(state);
    if (scroll == null) return const _Spinner();
    final split = _splitOf(messages);
    _scheduleChecks();

    Widget rowAt(int index) {
      final m = messages[index];
      return _TrackedMessage(
        key: ValueKey(m.id),
        id: m.id,
        sent: m.state == PrivateSendState.sent,
        registry: _onScreen,
        child: _row(
          context,
          m,
          index,
          messages,
          compact,
          peerName,
          contact?.relayServerKey ?? '',
          links,
          colors,
        ),
      );
    }

    final olderIndex = {
      for (var i = 0; i < split; i++) messages[i].id: split - 1 - i,
    };
    final newerIndex = {
      for (var i = split; i < messages.length; i++) messages[i].id: i - split,
    };
    final loadingOlder = state.loading;
    return CustomScrollView(
      controller: scroll,
      center: _newerKey,
      anchor: 1,
      slivers: [
        SliverPadding(
          padding: const EdgeInsets.fromLTRB(
            OcSpace.s16,
            OcSpace.s12,
            OcSpace.s16,
            0,
          ),
          sliver: SliverList.builder(
            itemCount: split + (loadingOlder ? 1 : 0),
            findChildIndexCallback: (key) =>
                olderIndex[(key as ValueKey<Object>).value],
            itemBuilder: (context, index) =>
                index == split ? const _Spinner() : rowAt(split - 1 - index),
          ),
        ),
        SliverPadding(
          key: _newerKey,
          padding: const EdgeInsets.fromLTRB(
            OcSpace.s16,
            0,
            OcSpace.s16,
            OcSpace.s12,
          ),
          sliver: SliverList.builder(
            itemCount: messages.length - split,
            findChildIndexCallback: (key) =>
                newerIndex[(key as ValueKey<Object>).value],
            itemBuilder: (context, index) => rowAt(split + index),
          ),
        ),
      ],
    );
  }

  Widget _row(
    BuildContext context,
    PrivateMessage m,
    int index,
    List<PrivateMessage> messages,
    bool compact,
    String peerName,
    String relayServerKey,
    MarkdownContext links,
    OcColors colors,
  ) => switch (m.kind) {
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
    PrivateMessageKind.keysChanged => _keyLine(
      context,
      OcIcons.key,
      m.outgoing ? 'Your keys changed.' : "$peerName's keys changed.",
    ),
    PrivateMessageKind.newDevice => _keyLine(
      context,
      OcIcons.lock,
      '$peerName signed in on a new device.',
    ),
    PrivateMessageKind.unacceptedKeys => _keyLine(
      context,
      OcIcons.warning,
      "A message from keys you haven't accepted.",
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
      index,
      messages,
      compact,
      peerName,
      relayServerKey,
      links,
    ),
  };

  /// A quiet line about the chat's keys (design 2026-11-01 §8.2).
  Widget _keyLine(BuildContext context, IconData icon, String text) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: OcSpace.s8),
      child: Center(
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 14, color: colors.textMuted),
            const SizedBox(width: OcSpace.s6),
            Flexible(
              child: Text(
                text,
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ),
          ],
        ),
      ),
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

class _Spinner extends StatelessWidget {
  const _Spinner();

  @override
  Widget build(BuildContext context) => const Center(
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

/// A message row that knows where it is, so the list can tell which is at
/// the top.
class _TrackedMessage extends StatefulWidget {
  const _TrackedMessage({
    super.key,
    required this.id,
    required this.sent,
    required this.registry,
    required this.child,
  });

  final String id;

  /// Only a sent message is kept as where the reader was.
  final bool sent;
  final Map<String, _TrackedMessageState> registry;
  final Widget child;

  @override
  State<_TrackedMessage> createState() => _TrackedMessageState();
}

class _TrackedMessageState extends State<_TrackedMessage> {
  @override
  void initState() {
    super.initState();
    widget.registry[widget.id] = this;
  }

  @override
  void didUpdateWidget(_TrackedMessage oldWidget) {
    super.didUpdateWidget(oldWidget);
    _forget(oldWidget);
    widget.registry[widget.id] = this;
  }

  @override
  void dispose() {
    _forget(widget);
    super.dispose();
  }

  void _forget(_TrackedMessage from) {
    if (identical(from.registry[from.id], this)) from.registry.remove(from.id);
  }

  Rect? rectIn(RenderBox ancestor) {
    final box = context.findRenderObject();
    if (box is! RenderBox || !box.attached || !box.hasSize) return null;
    return box.localToGlobal(Offset.zero, ancestor: ancestor) & box.size;
  }

  @override
  Widget build(BuildContext context) => widget.child;
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
