import 'dart:async';

import 'package:clock/clock.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/features/chat/hover_bar.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/features/chat/message_line.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// Gives a message row's bubble the text selected in it, so the context
/// menu can offer to copy just that.
typedef SelectionChanged = void Function(String? selected);

/// A message row and what can be done to it (§4.5, §16):
///
/// * a hover bar beside the bubble, gone as soon as the pointer leaves the
///   row and the bar, as Discord's is;
/// * double-click to reply, except on buttons and links inside it;
/// * the context menu by right-click, on the message or one of its files
///   ([AttachmentTarget]), or Shift+F10 and the Menu key while
///   the row has keyboard focus.
///
/// Clicks are watched with a [Listener] rather than gesture recognizers, so
/// taps on reaction chips, links and quotes inside are never delayed.
class MessageItem extends StatefulWidget {
  const MessageItem({
    super.key,
    required this.message,
    required this.own,
    required this.first,
    required this.actions,
    required this.builder,
    this.avatar,
    this.compact = false,
  });

  final Message message;
  final bool own;
  final bool first;
  final MessageActions actions;
  final Widget? avatar;

  /// Builds the bubble, or the whole line in compact density, reporting
  /// text selected inside it.
  final Widget Function(SelectionChanged onSelectionChanged) builder;

  /// Compact density (§8.1): a line without a bubble; the bar sits on its
  /// top edge at the right, like Discord's.
  final bool compact;

  @override
  State<MessageItem> createState() => _MessageItemState();
}

class _MenuIntent extends Intent {
  const _MenuIntent();
}

/// Whether the pointer is on a message row or its hover bar, for rows that
/// change while hovered (compact density shows the time and a background).
class MessageHover extends InheritedNotifier<ValueNotifier<bool>> {
  const MessageHover({
    super.key,
    required ValueNotifier<bool> hovered,
    required super.child,
  }) : super(notifier: hovered);

  static bool of(BuildContext context) =>
      context
          .dependOnInheritedWidgetOfExactType<MessageHover>()
          ?.notifier
          ?.value ??
      false;
}

/// Where the hover bar sits relative to the bubble.
typedef _Placement = ({Alignment target, Alignment follower, Offset offset});

class _MessageItemState extends State<MessageItem> {
  final _link = LayerLink();
  final _bubbleKey = GlobalKey();
  final _bar = OverlayPortalController();
  final _hovered = ValueNotifier(false);
  _Placement? _placement;
  bool _overRow = false;
  bool _overBar = false;
  bool _focusRing = false;
  String? _selection;

  DateTime? _lastDown;
  Offset? _lastDownAt;
  Offset? _secondaryDown;

  Message get _message => widget.message;

  bool get _hasBar =>
      _message.sendState != SendState.pending &&
      !_message.isSystem &&
      _barFor(_message).buttons > 0;

  @override
  void dispose() {
    _hovered.dispose();
    super.dispose();
  }

  void _show() {
    _hovered.value = true;
    if (!_hasBar || _bar.isShowing) return;
    _placement = _place();
    _bar.show();
  }

  /// React, Edit and Reply when they apply; More only on compact lines,
  /// since a bubble's menu is a right-click away.
  HoverActionBar _barFor(Message message) {
    final actions = widget.actions;
    return HoverActionBar(
      onReact: actions.canReact(message)
          ? (anchor) => actions.react(message, anchor: anchor)
          : null,
      onEdit: widget.compact && actions.canEdit(message)
          ? () => actions.edit(message)
          : null,
      onReply: actions.canReply(message) ? () => actions.reply(message) : null,
      onMore: widget.compact
          ? (anchor) => actions.showMenu(
              message,
              position: anchor.bottomLeft,
              selection: _selection,
            )
          : null,
    );
  }

  int get _buttons => _barFor(_message).buttons;

  /// Beside the bubble on its outer side when there is room, otherwise on
  /// its top corner; against its bottom when its top has scrolled away.
  _Placement _place() {
    if (widget.compact) return _placeOnLine();
    final own = widget.own;
    final overlay = Overlay.of(context).context.findRenderObject();
    final bubble = _bubbleKey.currentContext?.findRenderObject();
    var outside = true;
    var bottom = false;
    if (overlay is RenderBox && bubble is RenderBox && bubble.hasSize) {
      final rect =
          bubble.localToGlobal(Offset.zero, ancestor: overlay) & bubble.size;
      final room = own ? rect.left : overlay.size.width - rect.right;
      outside =
          room >= HoverActionBar.widthFor(_buttons) + OcSpace.s6 + OcSpace.s4;
      // Beside the bubble the bar starts at its top; on the edge it rises
      // above it.
      final above = outside
          ? 0.0
          : HoverActionBar.height - HoverActionBar.overlap;
      bottom = rect.top < above + OcSpace.s4;
    }
    final side = own ? -1.0 : 1.0;
    if (outside) {
      return (
        target: Alignment(side, bottom ? 1 : -1),
        follower: Alignment(-side, bottom ? 1 : -1),
        offset: Offset(side * OcSpace.s6, 0),
      );
    }
    // Straddling the bubble's edge, tucked into its outer corner.
    return (
      target: Alignment(side, bottom ? 1 : -1),
      follower: Alignment(side, bottom ? -1 : 1),
      offset: Offset(
        -side * OcSpace.s8,
        bottom ? -HoverActionBar.overlap : HoverActionBar.overlap,
      ),
    );
  }

  /// Hides the bar once the pointer event that left has been handled:
  /// leaving the row for its bar enters the bar in the same event.
  void _scheduleHide() {
    scheduleMicrotask(() {
      if (!mounted || _overRow || _overBar) return;
      _hovered.value = false;
      if (_bar.isShowing) _bar.hide();
    });
  }

  HitTestResult _hitTest(Offset position) {
    final result = HitTestResult();
    WidgetsBinding.instance.hitTestInView(
      result,
      position,
      View.of(context).viewId,
    );
    return result;
  }

  /// The file at [position], if any ([AttachmentTarget]).
  Attachment? _attachmentAt(Offset position) {
    for (final entry in _hitTest(position).path) {
      if (entry.target case RenderMetaData(
        metaData: final Attachment attachment,
      )) {
        return attachment;
      }
    }
    return null;
  }

  /// Whether [position] is on something clickable inside the message: a
  /// reaction chip, a link, the reply quote.
  bool _onControl(Offset position) {
    return _hitTest(position).path.any(
      (entry) =>
          entry.target is MouseTrackerAnnotation &&
          (entry.target as MouseTrackerAnnotation).cursor ==
              SystemMouseCursors.click,
    );
  }

  void _onPointerDown(PointerDownEvent event) {
    if (event.buttons == kSecondaryMouseButton) {
      _secondaryDown = event.position;
      return;
    }
    if (event.buttons != kPrimaryButton) return;
    // The clock rather than event time stamps, which tests leave at zero.
    final now = clock.now();
    final last = _lastDown;
    final quick =
        last != null &&
        now.difference(last) <= kDoubleTapTimeout &&
        (event.position - _lastDownAt!).distance <= kDoubleTapSlop;
    _lastDown = now;
    _lastDownAt = event.position;
    if (quick && !_onControl(event.position)) {
      _lastDown = null;
      if (widget.actions.canReply(_message)) widget.actions.reply(_message);
    }
  }

  void _onPointerUp(PointerUpEvent event) {
    final down = _secondaryDown;
    _secondaryDown = null;
    if (down == null || (event.position - down).distance > kTouchSlop) return;
    // A file's menu is the message's, with the file's own items first.
    final attachment = _attachmentAt(event.position);
    if (attachment == null && _onControl(event.position)) return;
    widget.actions.showMenu(
      _message,
      position: event.position,
      selection: _selection,
      attachment: attachment,
    );
  }

  void _menuFromKeyboard() {
    final box = context.findRenderObject()! as RenderBox;
    widget.actions.showMenu(
      _message,
      position: box.localToGlobal(box.size.center(Offset.zero)),
      selection: _selection,
    );
  }

  /// On the line's top edge at the right; against its bottom when the top
  /// has scrolled away.
  _Placement _placeOnLine() {
    final overlay = Overlay.of(context).context.findRenderObject();
    final line = _bubbleKey.currentContext?.findRenderObject();
    var bottom = false;
    if (overlay is RenderBox && line is RenderBox && line.hasSize) {
      final top = line.localToGlobal(Offset.zero, ancestor: overlay).dy;
      bottom =
          top < HoverActionBar.height - HoverActionBar.overlap + OcSpace.s4;
    }
    return (
      target: Alignment(1, bottom ? 1 : -1),
      follower: Alignment(1, bottom ? -1 : 1),
      offset: Offset(
        -OcSpace.s16,
        bottom ? -HoverActionBar.overlap : HoverActionBar.overlap,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final message = _message;
    final colors = context.oc;
    final anchored = CompositedTransformTarget(
      link: _link,
      child: KeyedSubtree(
        key: _bubbleKey,
        child: widget.builder((selected) => _selection = selected),
      ),
    );
    return OverlayPortal(
      controller: _bar,
      overlayChildBuilder: (context) => Positioned(
        left: 0,
        top: 0,
        child: CompositedTransformFollower(
          link: _link,
          showWhenUnlinked: false,
          targetAnchor: _placement?.target ?? Alignment.topRight,
          followerAnchor: _placement?.follower ?? Alignment.topLeft,
          offset: _placement?.offset ?? Offset.zero,
          child: MouseRegion(
            onEnter: (_) {
              _overBar = true;
              _show();
            },
            onExit: (_) {
              _overBar = false;
              _scheduleHide();
            },
            child: _barFor(message),
          ),
        ),
      ),
      child: FocusableActionDetector(
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.f10, shift: true): _MenuIntent(),
          SingleActivator(LogicalKeyboardKey.contextMenu): _MenuIntent(),
        },
        actions: {
          _MenuIntent: CallbackAction<_MenuIntent>(
            onInvoke: (_) => _menuFromKeyboard(),
          ),
        },
        onShowFocusHighlight: (value) => setState(() => _focusRing = value),
        child: MouseRegion(
          onEnter: (_) {
            _overRow = true;
            _show();
          },
          onExit: (_) {
            _overRow = false;
            _scheduleHide();
          },
          child: Listener(
            onPointerDown: _onPointerDown,
            onPointerUp: _onPointerUp,
            child: DecoratedBox(
              position: DecorationPosition.foreground,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(OcRadius.bubble),
                border: _focusRing
                    ? Border.all(color: colors.text, width: 1.5)
                    : null,
              ),
              child: widget.compact
                  ? MessageHover(hovered: _hovered, child: anchored)
                  : MessageLine(
                      own: widget.own,
                      first: widget.first,
                      avatar: widget.avatar,
                      bubble: anchored,
                    ),
            ),
          ),
        ),
      ),
    );
  }
}
