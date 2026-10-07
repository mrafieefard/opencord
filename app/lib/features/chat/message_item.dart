import 'dart:async';

import 'package:clock/clock.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';

import 'package:opencord/core/model/message.dart';
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
/// * a hover bar beside the bubble, which lingers 150 ms after the pointer
///   leaves so it can be reached;
/// * double-click to reply, except on buttons and links inside it;
/// * the context menu by right-click, or Shift+F10 and the Menu key while
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
    required this.bubbleBuilder,
    this.avatar,
  });

  final Message message;
  final bool own;
  final bool first;
  final MessageActions actions;
  final Widget? avatar;

  /// Builds the bubble, reporting text selected inside it.
  final Widget Function(SelectionChanged onSelectionChanged) bubbleBuilder;

  static const hideDelay = Duration(milliseconds: 150);

  @override
  State<MessageItem> createState() => _MessageItemState();
}

class _MenuIntent extends Intent {
  const _MenuIntent();
}

/// Where the hover bar sits relative to the bubble.
typedef _Placement = ({Alignment target, Alignment follower, Offset offset});

class _MessageItemState extends State<MessageItem> {
  final _link = LayerLink();
  final _bubbleKey = GlobalKey();
  final _bar = OverlayPortalController();
  _Placement? _placement;
  Timer? _hide;
  bool _overRow = false;
  bool _overBar = false;
  bool _focusRing = false;
  String? _selection;

  DateTime? _lastDown;
  Offset? _lastDownAt;
  Offset? _secondaryDown;

  Message get _message => widget.message;

  bool get _hasBar =>
      _message.sendState != SendState.pending && !_message.isSystem;

  @override
  void dispose() {
    _hide?.cancel();
    super.dispose();
  }

  void _show() {
    _hide?.cancel();
    if (!_hasBar || _bar.isShowing) return;
    _placement = _place();
    _bar.show();
  }

  int get _buttons =>
      1 +
      (widget.actions.canReact(_message) ? 1 : 0) +
      (widget.actions.canReply(_message) ? 1 : 0);

  /// Beside the bubble on its outer side when there is room, otherwise on
  /// its top corner; against its bottom when its top has scrolled away.
  _Placement _place() {
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
      bottom = rect.top < OcSpace.s4;
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
      offset: Offset(-side * OcSpace.s8, bottom ? -14 : 14),
    );
  }

  void _scheduleHide() {
    _hide?.cancel();
    _hide = Timer(MessageItem.hideDelay, () {
      if (mounted && !_overRow && !_overBar && _bar.isShowing) _bar.hide();
    });
  }

  /// Whether [position] is on something clickable inside the message: a
  /// reaction chip, a link, the reply quote.
  bool _onControl(Offset position) {
    final result = HitTestResult();
    WidgetsBinding.instance.hitTestInView(
      result,
      position,
      View.of(context).viewId,
    );
    return result.path.any(
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
    if (_onControl(event.position)) return;
    widget.actions.showMenu(
      _message,
      position: event.position,
      selection: _selection,
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

  @override
  Widget build(BuildContext context) {
    final actions = widget.actions;
    final message = _message;
    final colors = context.oc;
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
            child: HoverActionBar(
              onReact: actions.canReact(message)
                  ? (anchor) => actions.react(message, anchor: anchor)
                  : null,
              onReply: actions.canReply(message)
                  ? () => actions.reply(message)
                  : null,
              onMore: (anchor) => actions.showMenu(
                message,
                position: anchor.bottomLeft,
                selection: _selection,
              ),
            ),
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
              child: MessageLine(
                own: widget.own,
                first: widget.first,
                avatar: widget.avatar,
                bubble: CompositedTransformTarget(
                  link: _link,
                  child: KeyedSubtree(
                    key: _bubbleKey,
                    child: widget.bubbleBuilder(
                      (selected) => _selection = selected,
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
