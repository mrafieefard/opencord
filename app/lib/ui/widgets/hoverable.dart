import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// What the pointer and keyboard are doing to a [Hoverable].
@immutable
class HoverState {
  const HoverState({
    this.hovered = false,
    this.pressed = false,
    this.focused = false,
  });

  final bool hovered;
  final bool pressed;
  final bool focused;

  /// Hovered or pressed: the moments a row or button lights up.
  bool get active => hovered || pressed;
}

/// The base of every interactive surface: hover and press state, tap,
/// double tap and secondary tap (context menu), all reachable from the
/// keyboard. Enter or Space taps; Shift+F10 or the Menu key opens the
/// context menu at the widget's center. Keyboard focus draws a 1.5 px
/// ring (§9).
class Hoverable extends StatefulWidget {
  const Hoverable({
    super.key,
    required this.builder,
    this.onTap,
    this.onDoubleTap,
    this.onSecondaryTap,
    this.cursor = SystemMouseCursors.click,
    this.focusNode,
    this.autofocus = false,
    this.focusRadius = const BorderRadius.all(Radius.circular(OcRadius.row)),
    this.semanticLabel,
    this.selected,
    this.toggled,
    this.button = true,
    this.showFocusRing = true,
  });

  /// Lets tests find the focus ring.
  static const focusRingKey = Key('hoverable-focus-ring');

  final Widget Function(BuildContext context, HoverState state) builder;
  final VoidCallback? onTap;
  final VoidCallback? onDoubleTap;

  /// Receives the global position the menu should open at.
  final ValueChanged<Offset>? onSecondaryTap;
  final MouseCursor cursor;
  final FocusNode? focusNode;
  final bool autofocus;
  final BorderRadius focusRadius;
  final String? semanticLabel;
  final bool? selected;

  /// On or off, for a switch: read out instead of "button".
  final bool? toggled;

  /// Whether screen readers announce it as a button.
  final bool button;

  /// Off for surfaces that show focus themselves, like menu items.
  final bool showFocusRing;

  @override
  State<Hoverable> createState() => _HoverableState();
}

class _ContextMenuIntent extends Intent {
  const _ContextMenuIntent();
}

class _HoverableState extends State<Hoverable> {
  bool _hovered = false;
  bool _pressed = false;
  bool _focused = false;
  bool _showFocusRing = false;

  bool get _interactive =>
      widget.onTap != null ||
      widget.onDoubleTap != null ||
      widget.onSecondaryTap != null;

  void _set(VoidCallback change) {
    if (mounted) setState(change);
  }

  Offset _center() {
    final box = context.findRenderObject()! as RenderBox;
    return box.localToGlobal(box.size.center(Offset.zero));
  }

  /// With a [Hoverable.semanticLabel], the label speaks for the content, so
  /// the visible text is not read twice.
  Widget _content(BuildContext context, HoverState state) {
    final content = widget.builder(context, state);
    return widget.semanticLabel == null
        ? content
        : ExcludeSemantics(child: content);
  }

  @override
  Widget build(BuildContext context) {
    final state = HoverState(
      hovered: _hovered,
      pressed: _pressed,
      focused: _focused,
    );
    final onSecondaryTap = widget.onSecondaryTap;
    return Semantics(
      button: widget.button && widget.onTap != null && widget.toggled == null,
      label: widget.semanticLabel,
      selected: widget.selected,
      toggled: widget.toggled,
      child: MouseRegion(
        cursor: _interactive ? widget.cursor : MouseCursor.defer,
        onEnter: (_) => _set(() => _hovered = true),
        onExit: (_) => _set(() => _hovered = false),
        child: FocusableActionDetector(
          enabled: _interactive,
          focusNode: widget.focusNode,
          autofocus: widget.autofocus,
          onShowFocusHighlight: (value) =>
              _set(() => _showFocusRing = value && widget.showFocusRing),
          onFocusChange: (value) => _set(() => _focused = value),
          shortcuts: {
            if (onSecondaryTap != null) ...{
              const SingleActivator(LogicalKeyboardKey.f10, shift: true):
                  const _ContextMenuIntent(),
              const SingleActivator(LogicalKeyboardKey.contextMenu):
                  const _ContextMenuIntent(),
            },
          },
          actions: {
            if (widget.onTap != null)
              ActivateIntent: CallbackAction<ActivateIntent>(
                onInvoke: (_) => widget.onTap?.call(),
              ),
            if (onSecondaryTap != null)
              _ContextMenuIntent: CallbackAction<_ContextMenuIntent>(
                onInvoke: (_) => onSecondaryTap(_center()),
              ),
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: widget.onTap,
            onTapDown: widget.onTap == null
                ? null
                : (_) => _set(() => _pressed = true),
            onTapUp: widget.onTap == null
                ? null
                : (_) => _set(() => _pressed = false),
            onTapCancel: widget.onTap == null
                ? null
                : () => _set(() => _pressed = false),
            onDoubleTap: widget.onDoubleTap,
            onSecondaryTapUp: onSecondaryTap == null
                ? null
                : (details) => onSecondaryTap(details.globalPosition),
            child: _showFocusRing
                ? Stack(
                    children: [
                      _content(context, state),
                      Positioned.fill(
                        child: IgnorePointer(
                          child: DecoratedBox(
                            key: Hoverable.focusRingKey,
                            decoration: BoxDecoration(
                              borderRadius: widget.focusRadius,
                              border: Border.all(
                                color: context.oc.text,
                                width: 1.5,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ],
                  )
                : _content(context, state),
          ),
        ),
      ),
    );
  }
}
