import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/chat_row_view.dart';
import 'package:opencord/features/chat/chat_scroll.dart';
import 'package:opencord/features/chat/links.dart';
import 'package:opencord/features/chat/list_overlays.dart';
import 'package:opencord/features/chat/list_rows.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/features/chat/message_rows.dart';
import 'package:opencord/features/members/member_profile.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Where the reader left each channel, across restarts too (§16).
final scrollMemoryProvider = Provider<ScrollMemory>((ref) {
  final memory = ScrollMemory(ref.watch(keyValueStoreProvider));
  ref.onDispose(memory.flush);
  return memory;
});

/// One channel's history (§4.5, §6): newest at the bottom, older pages
/// loading near the top, the unread line, a floating date while
/// scrolling and a button back to the newest message.
class MessageList extends ConsumerStatefulWidget {
  const MessageList({
    super.key,
    required this.channel,
    required this.controller,
  });

  final ChannelRef channel;
  final ChatController controller;

  @override
  ConsumerState<MessageList> createState() => _MessageListState();
}

class _MessageListState extends ConsumerState<MessageList>
    implements MessageListHandle {
  static const _newerKey = ValueKey('newer');
  static const _highlightFor = Duration(milliseconds: 1600);
  static const _pillLinger = Duration(milliseconds: 1200);

  late final ActivityNotifier _activity;
  late final ScrollMemory _memory;
  ChatScrollController? _scroll;

  /// The first message of the newer half (see [splitIndex]).
  int _splitId = 0;

  /// Messages after this one get the unread line.
  int _lineReadId = 0;
  bool _hasLine = false;

  /// The newest message the reader has been at; newer ones count on the
  /// jump button.
  int _seenNewestId = 0;
  bool _reading = false;
  SavedScroll? _restoring;
  SavedScroll? _saved;

  final _onScreen = <Object, TrackedRowState>{};
  final _knownKeys = <Object>{};
  final _vanishing = <int, Message>{};
  List<ChatRow> _rows = const [];
  int _split = 0;
  int? _highlight;
  Timer? _highlightTimer;
  String? _pill;
  Timer? _pillTimer;
  bool _scrolling = false;
  bool _jumpVisible = false;
  bool _checksScheduled = false;

  ChannelRef get _channel => widget.channel;

  ChannelMessagesNotifier get _messages =>
      ref.read(channelMessagesProvider(_channel).notifier);

  /// The history this list shows, told when it is shown and hidden.
  late final ChannelMessagesNotifier _history;

  @override
  void initState() {
    super.initState();
    _activity = ref.read(activityProvider(_channel.server).notifier);
    _memory = ref.read(scrollMemoryProvider);
    _history = _messages;
    widget.controller.attach(this);
    // Not during the build that created this list.
    final recent = ref.read(recentChannelsProvider.notifier);
    final channel = _channel;
    final history = _history;
    scheduleMicrotask(() {
      recent.visit(channel);
      history.show();
    });
    ref.listenManual(
      windowStatusProvider.select((status) => status.focused),
      (_, _) => _scheduleChecks(),
    );
  }

  @override
  void didUpdateWidget(MessageList oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.controller, widget.controller)) {
      oldWidget.controller.detach(this);
      widget.controller.attach(this);
    }
  }

  @override
  void dispose() {
    widget.controller.detach(this);
    _memory.save(_channel, _saved);
    final activity = _activity;
    final channel = _channel.channel;
    final history = _history;
    // Not while the tree is being torn down: listeners would rebuild.
    scheduleMicrotask(() {
      activity.unfocus(channel);
      history.hide();
    });
    _scroll?.dispose();
    _highlightTimer?.cancel();
    _pillTimer?.cancel();
    super.dispose();
  }

  // Opening ----------------------------------------------------------------

  ChatScrollController _open(ChannelMessages state, int selfId) {
    final scroll = _start(state, selfId)..addListener(_scheduleChecks);
    if (widget.controller.takeJump(_channel) case final message?) {
      // A link to a message here (§15), once the list has its first layout.
      SchedulerBinding.instance.addPostFrameCallback((_) {
        if (mounted) jumpToMessage(message);
      });
    }
    return scroll;
  }

  /// Decides where the channel opens (§6): where the reader left it, else
  /// at the unread line, else at the newest message.
  ChatScrollController _start(ChannelMessages state, int selfId) {
    final read = ref
        .read(activityProvider(_channel.server))
        .of(_channel.channel)
        .read;
    final confirmed = state.messages;
    final newest = confirmed.isEmpty ? 0 : confirmed.last.id;
    _lineReadId = read.unread > 0 ? read.lastReadId : newest;
    _seenNewestId = _lineReadId;
    final saved = _memory.read(_channel);
    if (saved != null && confirmed.any((m) => m.id == saved.messageId)) {
      _splitId = saved.messageId;
      _restoring = saved;
      return ChatScrollController(
        start: StartAtAnchor(saved.fromTop),
        wheel: OcMotion.of(context).wheel,
      );
    }
    final rows = buildRows(
      state.all,
      selfId: selfId,
      lastReadId: _lineReadId,
      reachedStart: !state.hasOlder,
    );
    final line = rows.indexWhere((row) => row is UnreadRow);
    final firstUnread = line == -1 ? null : messageOf(rows[line + 1]);
    if (firstUnread != null) {
      _splitId = firstUnread.id;
      return ChatScrollController(
        start: const StartAtAnchor(0, fraction: 0.15),
        wheel: OcMotion.of(context).wheel,
      );
    }
    _splitId = newest + 1;
    return ChatScrollController(wheel: OcMotion.of(context).wheel);
  }

  // Building ---------------------------------------------------------------

  /// Keeps deleted messages for their fade-out (§6).
  void _onMessagesChanged(ChannelMessages? previous, ChannelMessages next) {
    _announce(previous, next);
    if (previous == null || OcMotion.of(context).message == Duration.zero) {
      return;
    }
    final kept = {for (final message in next.messages) message.id};
    // Back again (a newer page put it there): no longer going.
    _vanishing.removeWhere((id, _) => kept.contains(id));
    for (final message in previous.messages) {
      if (kept.contains(message.id)) continue;
      _vanishing[message.id] = message;
      Timer(OcMotion.of(context).message, () {
        if (mounted) setState(() => _vanishing.remove(message.id));
      });
    }
  }

  /// Messages from others arriving here are read out politely (§9);
  /// history loading in is not.
  void _announce(ChannelMessages? previous, ChannelMessages next) {
    if (previous == null || !previous.loaded) return;
    final data = ref.read(serverProvider(_channel.server)).data;
    if (data == null) return;
    final newest = previous.messages.lastOrNull?.id;
    final arrived = [
      for (final message in next.messages)
        if ((newest == null || message.id > newest) &&
            message.authorId != data.self.id &&
            !message.isSystem)
          message,
    ];
    if (arrived.isEmpty) return;
    final view = View.of(context);
    final direction = Directionality.of(context);
    const spoken = 3;
    for (final message in arrived.take(spoken)) {
      final author = data.members[message.authorId]?.displayName ?? 'Someone';
      final text = copyableText(
        message.content,
        user: (id) => data.members[id]?.displayName,
        channel: (id) => data.channels[id]?.name,
      );
      SemanticsService.sendAnnouncement(view, '$author: $text', direction);
    }
    if (arrived.length > spoken) {
      SemanticsService.sendAnnouncement(
        view,
        '${arrived.length - spoken} more new messages',
        direction,
      );
    }
  }

  List<Message> _withVanishing(ChannelMessages state) {
    if (_vanishing.isEmpty) return state.all;
    final shown = {for (final message in state.messages) message.id};
    final merged = [
      ...state.messages,
      for (final message in _vanishing.values)
        if (!shown.contains(message.id)) message,
    ]..sort((a, b) => a.id.compareTo(b.id));
    return [...merged, ...state.pending];
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(channelMessagesProvider(_channel), _onMessagesChanged);
    final state = ref.watch(channelMessagesProvider(_channel));
    final data = ref.watch(
      serverProvider(_channel.server).select((s) => s.data),
    );
    final channel = data?.channels[_channel.channel];
    if (data == null || channel == null || !state.loaded) {
      if (state.loadError case final error? when data != null) {
        return _LoadFailed(
          reason: error.message,
          onRetry: () => _messages.reloadLatest(),
        );
      }
      return const Align(
        alignment: Alignment.bottomCenter,
        child: ChatSkeleton(),
      );
    }
    final scroll = _scroll ?? (_scroll = _open(state, data.self.id));
    final newest = state.messages.lastOrNull?.id ?? 0;
    if (_reading && !_hasLine) _lineReadId = math.max(_lineReadId, newest);
    final messages = _withVanishing(state);
    final rows = _rows = buildRows(
      messages,
      selfId: data.self.id,
      lastReadId: _lineReadId,
      reachedStart: !state.hasOlder,
    );
    _hasLine = rows.any((row) => row is UnreadRow);
    final split = _split = splitIndex(rows, _splitId);
    final fresh = {
      for (final row in rows.skip(split))
        if (_knownKeys.isNotEmpty && !_knownKeys.contains(row.key)) row.key,
    };
    _knownKeys.addAll(rows.map((row) => row.key));
    _scheduleChecks();

    final lookups = ChatLookups(data: data, messages: messages);
    final actions = _actions(data);
    final now = ref.read(clockProvider)();
    final reactions = ref.read(repositoryProvider).capabilities.reactions;
    final compact =
        ref.watch(appSettingsProvider.select((settings) => settings.density)) ==
        MessageDensity.compact;
    Widget rowAt(int index) {
      final row = rows[index];
      final message = messageOf(row);
      return TrackedRow(
        key: ValueKey(row.key),
        row: row,
        registry: _onScreen,
        child: RowFade(
          fadeIn: fresh.contains(row.key),
          gone: message != null && _vanishing.containsKey(message.id),
          child: ChatRowView(
            row: row,
            lookups: lookups,
            actions: actions,
            channel: channel,
            now: now,
            reactions: reactions,
            compact: compact,
            highlighted: message != null && message.id == _highlight,
          ),
        ),
      );
    }

    final olderIndex = {
      for (var i = 0; i < split; i++) rows[i].key: split - 1 - i,
    };
    final newerIndex = {
      for (var i = split; i < rows.length; i++) rows[i].key: i - split,
    };
    final unseen = _unseen(state.messages, data.self.id);
    // Hover bars float in this overlay, clipped to the list (§4.5).
    return Overlay.wrap(
      child: LayoutBuilder(
        builder: (context, constraints) {
          final side = math.max(
            OcSpace.s16,
            (constraints.maxWidth - OcSize.messageColumn) / 2,
          );
          return Stack(
            children: [
              NotificationListener<ScrollNotification>(
                onNotification: _onScrollNotification,
                child: CustomScrollView(
                  controller: scroll,
                  center: _newerKey,
                  anchor: 1,
                  slivers: [
                    SliverPadding(
                      padding: EdgeInsets.fromLTRB(side, OcSpace.s8, side, 0),
                      sliver: SliverList.builder(
                        itemCount: split + (state.hasOlder ? 1 : 0),
                        findChildIndexCallback: (key) =>
                            olderIndex[(key as ValueKey<Object>).value],
                        itemBuilder: (context, index) => index == split
                            ? const OlderHistorySkeleton()
                            : rowAt(split - 1 - index),
                      ),
                    ),
                    SliverPadding(
                      key: _newerKey,
                      padding: EdgeInsets.fromLTRB(side, 0, side, OcSpace.s12),
                      sliver: SliverList.builder(
                        itemCount: rows.length - split,
                        findChildIndexCallback: (key) =>
                            newerIndex[(key as ValueKey<Object>).value],
                        itemBuilder: (context, index) => rowAt(split + index),
                      ),
                    ),
                  ],
                ),
              ),
              Positioned(
                top: OcSpace.s8,
                left: 0,
                right: 0,
                child: FloatingDayPill(label: _pill),
              ),
              Positioned(
                right: OcSpace.s16,
                bottom: OcSpace.s16,
                child: JumpToBottomButton(
                  visible:
                      _jumpVisible ||
                      (unseen > 0 && _laidOut(scroll) && !scroll.atEnd),
                  count: unseen,
                  onPressed: scrollToEnd,
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  ChatRowActions _actions(ServerData data) {
    final message = MessageActions(
      context: context,
      ref: ref,
      channel: _channel,
      controller: widget.controller,
    );
    return ChatRowActions(
      message: message,
      links: MarkdownContext(
        userName: (id) => data.members[id]?.displayName,
        channelName: (id) => data.channels[id]?.name,
        onLink: (url) => openMessageLink(context, ref, url),
        onUser: (id) =>
            showMemberProfile(context, serverKey: _channel.server, userId: id),
        onChannel: (id) {
          if (data.channels[id] != null) {
            ref
                .read(navigationProvider.notifier)
                .openChannel(_channel.server, id);
          }
        },
      ),
      onRetry: (message) {
        if (message.nonce case final nonce?) _messages.retry(nonce);
      },
      onReaction: (reacted, emoji) {
        if (reacted.sendState == SendState.sent) {
          message.toggleReaction(reacted, emoji);
        }
      },
      onJumpTo: jumpToMessage,
      onProfile: (id) =>
          showMemberProfile(context, serverKey: _channel.server, userId: id),
    );
  }

  /// Messages from others that arrived after the reader was last at the
  /// newest one.
  int _unseen(List<Message> messages, int selfId) {
    var count = 0;
    for (var i = messages.length - 1; i >= 0; i--) {
      final message = messages[i];
      if (message.id <= _seenNewestId) break;
      if (message.authorId != selfId) count++;
    }
    return count;
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
      final top = _topOf((message) => message.id == saved.messageId);
      if (top != null) position.jumpTo(position.pixels + top - saved.fromTop);
    }
    final state = ref.read(channelMessagesProvider(_channel));
    if (state.hasOlder &&
        !state.loadingOlder &&
        position.extentBefore < position.viewportDimension) {
      _messages.loadOlder();
    }
    final atEnd = position.atEnd;
    final newest = state.messages.lastOrNull?.id ?? 0;
    final seen = atEnd ? math.max(_seenNewestId, newest) : _seenNewestId;
    final jumpVisible = position.extentAfter > position.viewportDimension;
    if (seen != _seenNewestId || jumpVisible != _jumpVisible) {
      setState(() {
        _seenNewestId = seen;
        _jumpVisible = jumpVisible;
      });
    }
    _saved = atEnd ? null : _topPosition();
    _memory.save(_channel, _saved);
    if (_scrolling) {
      final label = _topDayLabel();
      if (label != _pill) setState(() => _pill = label);
    }
    final reading = atEnd && ref.read(windowStatusProvider).focused;
    if (reading != _reading) {
      _reading = reading;
      if (reading) {
        _activity.focus(_channel.channel);
      } else {
        _activity.unfocus(_channel.channel);
      }
    }
  }

  static bool _laidOut(ChatScrollController scroll) =>
      scroll.hasClients && scroll.position.hasContentDimensions;

  /// The floating date shows while scrolling and lingers a moment after.
  /// It is worked out in [_checks], once the rows have their new places.
  bool _onScrollNotification(ScrollNotification notification) {
    if (notification.depth != 0) return false;
    switch (notification) {
      case ScrollUpdateNotification():
        _scrolling = true;
        _pillTimer?.cancel();
      case ScrollEndNotification():
        _pillTimer?.cancel();
        _pillTimer = Timer(_pillLinger, () {
          _scrolling = false;
          if (mounted && _pill != null) setState(() => _pill = null);
        });
      default:
        break;
    }
    return false;
  }

  RenderBox? get _box {
    final box = context.findRenderObject();
    return box is RenderBox && box.hasSize ? box : null;
  }

  /// Rows in view, top to bottom.
  List<(TrackedRowState, Rect)> _visibleRows() {
    final box = _box;
    if (box == null) return const [];
    final visible = <(TrackedRowState, Rect)>[];
    for (final tracked in _onScreen.values) {
      final rect = tracked.rectIn(box);
      if (rect != null && rect.bottom > 0 && rect.top < box.size.height) {
        visible.add((tracked, rect));
      }
    }
    return visible..sort((a, b) => a.$2.top.compareTo(b.$2.top));
  }

  /// The date of the topmost row, unless that row is its own date pill.
  String? _topDayLabel() {
    final scroll = _scroll;
    if (scroll == null || scroll.atEnd) return null;
    for (final (tracked, rect) in _visibleRows()) {
      if (rect.bottom <= OcSpace.s32) continue;
      final row = tracked.row;
      if (row is DayRow) return null;
      final day = dayOf(row);
      return day == null ? null : dayLabel(day, ref.read(clockProvider)());
    }
    return null;
  }

  double? _topOf(bool Function(Message message) test) {
    for (final (tracked, rect) in _visibleRows()) {
      final message = messageOf(tracked.row);
      if (message != null && test(message)) return rect.top;
    }
    for (final tracked in _onScreen.values) {
      final message = messageOf(tracked.row);
      if (message == null || !test(message)) continue;
      final box = _box;
      return box == null ? null : tracked.rectIn(box)?.top;
    }
    return null;
  }

  SavedScroll? _topPosition() {
    for (final (tracked, rect) in _visibleRows()) {
      final message = messageOf(tracked.row);
      if (message != null && message.sendState == SendState.sent) {
        return SavedScroll(messageId: message.id, fromTop: rect.top);
      }
    }
    return null;
  }

  // What the rest of the chat area asks for --------------------------------

  @override
  Future<void> scrollToEnd() async {
    await _scroll?.scrollToEnd(duration: OcMotion.of(context).morph);
  }

  @override
  void scrollPage(int direction) {
    final scroll = _scroll;
    if (scroll == null || !scroll.hasClients) return;
    final position = scroll.position;
    final target =
        (position.pixels + direction * position.viewportDimension * 0.85).clamp(
          position.minScrollExtent,
          position.maxScrollExtent,
        );
    final duration = OcMotion.of(context).morph;
    if (duration == Duration.zero) {
      position.jumpTo(target);
    } else {
      position.animateTo(target, duration: duration, curve: OcMotion.curve);
    }
  }

  @override
  Future<void> jumpToMessage(int messageId) async {
    bool loaded() => ref
        .read(channelMessagesProvider(_channel))
        .messages
        .any((message) => message.id == messageId);
    for (var page = 0; page < 20 && !loaded(); page++) {
      if (!ref.read(channelMessagesProvider(_channel)).hasOlder) break;
      await _messages.loadOlder();
      if (!mounted) return;
    }
    if (!loaded()) {
      showOcToast(context, 'That message is not available any more.');
      return;
    }
    for (var attempt = 0; attempt < 8; attempt++) {
      await SchedulerBinding.instance.endOfFrame;
      if (!mounted) return;
      final tracked = _onScreen.values
          .where((row) => messageOf(row.row)?.id == messageId)
          .firstOrNull;
      if (tracked != null) {
        await _reveal(tracked);
        _flash(messageId);
        return;
      }
      _jumpNear(messageId);
    }
  }

  /// Scrolls a built row to 30 % from the top. Scrolling forward moves rows
  /// up, so the distance is simply how far the row is from that line.
  /// (`Scrollable.ensureVisible` misplaces rows above a centered sliver.)
  Future<void> _reveal(TrackedRowState tracked) async {
    final scroll = _scroll;
    final box = _box;
    final rect = box == null ? null : tracked.rectIn(box);
    if (scroll == null || !scroll.hasClients || box == null || rect == null) {
      return;
    }
    final position = scroll.position;
    final target = (position.pixels + rect.top - box.size.height * 0.3).clamp(
      position.minScrollExtent,
      position.maxScrollExtent,
    );
    final duration = OcMotion.of(context).morph;
    if (duration == Duration.zero) {
      position.jumpTo(target);
    } else {
      await position.animateTo(
        target,
        duration: duration,
        curve: OcMotion.curve,
      );
    }
  }

  /// Jumps to where [messageId] should be, from the average row height, so
  /// that its row gets built.
  void _jumpNear(int messageId) {
    final scroll = _scroll;
    final box = _box;
    if (scroll == null || !scroll.hasClients || box == null) return;
    final index = _rows.indexWhere((row) => messageOf(row)?.id == messageId);
    if (index == -1) return;
    final heights = [
      for (final tracked in _onScreen.values) ?tracked.rectIn(box)?.height,
    ];
    final average = heights.isEmpty
        ? 60.0
        : heights.reduce((a, b) => a + b) / heights.length;
    final position = scroll.position;
    final view = position.viewportDimension;
    final target = index < _split
        ? view * 0.7 - (_split - index) * average
        : view * 0.7 + (index - _split) * average;
    position.jumpTo(
      target.clamp(position.minScrollExtent, position.maxScrollExtent),
    );
  }

  void _flash(int messageId) {
    _highlightTimer?.cancel();
    setState(() => _highlight = messageId);
    _highlightTimer = Timer(_highlightFor, () {
      if (mounted) setState(() => _highlight = null);
    });
  }
}

/// A channel whose newest page could not load (§4.13): why, and Try
/// again. It also loads by itself once the server is back.
class _LoadFailed extends StatelessWidget {
  const _LoadFailed({required this.reason, required this.onRetry});

  final String reason;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 360),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(OcIcons.cloudOff, size: 40, color: colors.textMuted),
              const SizedBox(height: OcSpace.s12),
              Text(
                "Couldn't load messages",
                textAlign: TextAlign.center,
                style: OcText.header.copyWith(color: colors.text),
              ),
              const SizedBox(height: OcSpace.s6),
              Text(
                reason,
                textAlign: TextAlign.center,
                style: OcText.body.copyWith(color: colors.textSecondary),
              ),
              const SizedBox(height: OcSpace.s16),
              OcButton(label: 'Try again', onPressed: onRetry),
            ],
          ),
        ),
      ),
    );
  }
}
