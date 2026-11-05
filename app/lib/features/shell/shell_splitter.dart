import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// The key under which the user's custom channel list width is persisted.
const sidebarWidthKey = 'ui.sidebarWidth';

/// Holds the saved or active width of the left column (§5.7).
///
/// `null` means the layout class's default (288 px for wide/medium, 264 px for
/// narrow).
class SidebarWidthNotifier extends Notifier<double?> {
  static const key = sidebarWidthKey;

  @override
  double? build() {
    final store = ref.watch(keyValueStoreProvider);
    final raw = store.read(key);
    if (raw == null) return null;
    return double.tryParse(raw);
  }

  /// Sets the live width during dragging without writing to persistent storage.
  void setLive(double? width) {
    state = width;
  }

  /// Saves the width and writes it to persistent storage once.
  void save(double? width) {
    state = width;
    final store = ref.read(keyValueStoreProvider);
    if (width == null) {
      store.write(key, null);
    } else {
      store.write(key, width.toString());
    }
  }

  /// Resets to default (null) and removes the key from storage.
  void reset() => save(null);
}

/// The provider for the user's channel list width.
final sidebarWidthProvider = NotifierProvider<SidebarWidthNotifier, double?>(
  SidebarWidthNotifier.new,
);

/// Expand the hit test area horizontally by [expansion] on both sides without
/// altering layout width or child positioning.
class _SplitterHitBox extends SingleChildRenderObjectWidget {
  const _SplitterHitBox({super.child, this.expansion = 4.0});

  final double expansion;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderSplitterHitBox(expansion);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderSplitterHitBox renderObject,
  ) {
    renderObject.expansion = expansion;
  }
}

class _RenderSplitterHitBox extends RenderProxyBox {
  _RenderSplitterHitBox(this.expansion);

  double expansion;

  @override
  bool hitTest(BoxHitTestResult result, {required Offset position}) {
    final hitRect = Rect.fromLTRB(
      -expansion,
      0,
      size.width + expansion,
      size.height,
    );
    if (hitRect.contains(position)) {
      final clamped = Offset(
        position.dx.clamp(0.0, size.width),
        position.dy.clamp(0.0, size.height),
      );
      if (hitTestChildren(result, position: clamped) || hitTestSelf(position)) {
        result.add(BoxHitTestEntry(this, position));
        return true;
      }
    }
    return false;
  }
}

/// A draggable splitter between the left column and main float (§5.7).
///
/// Sits in the 8 px gap with a 16 px hit area (gap + 4 on each side).
/// Shows a 3 × 40 pill while hovered, focused or dragging.
class ShellSplitter extends StatefulWidget {
  const ShellSplitter({
    super.key = ShellKeys.splitter,
    required this.currentWidth,
    required this.minWidth,
    required this.maxWidth,
    required this.defaultWidth,
    required this.isDragging,
    required this.onDragStart,
    required this.onDragDelta,
    required this.onDragEnd,
    required this.onReset,
    required this.onStep,
    this.focusNode,
  });

  final double currentWidth;
  final double minWidth;
  final double maxWidth;
  final double defaultWidth;
  final bool isDragging;
  final VoidCallback onDragStart;
  final ValueChanged<double> onDragDelta;
  final VoidCallback onDragEnd;
  final VoidCallback onReset;
  final ValueChanged<double> onStep;
  final FocusNode? focusNode;

  @override
  State<ShellSplitter> createState() => _ShellSplitterState();
}

class _ShellSplitterState extends State<ShellSplitter> {
  bool _isHovered = false;
  bool _isFocused = false;
  bool _dragStarted = false;
  DateTime? _lastTapTime;
  FocusNode? _internalFocusNode;

  FocusNode get _effectiveFocusNode =>
      widget.focusNode ?? (_internalFocusNode ??= FocusNode());

  @override
  void dispose() {
    _internalFocusNode?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final active = _isHovered || _isFocused || widget.isDragging;

    return SizedBox(
      width: OcFloats.gap,
      child: _SplitterHitBox(
        expansion: 4.0,
        child: Focus(
          focusNode: _effectiveFocusNode,
          onKeyEvent: (node, event) {
            if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
              return KeyEventResult.ignored;
            }
            if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
              widget.onStep(-16);
              return KeyEventResult.handled;
            }
            if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
              widget.onStep(16);
              return KeyEventResult.handled;
            }
            if (event.logicalKey == LogicalKeyboardKey.home) {
              widget.onReset();
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          onFocusChange: (focused) {
            if (_isFocused != focused) {
              setState(() => _isFocused = focused);
            }
          },
          child: Semantics(
            slider: true,
            label: 'Resize the channel list',
            value: '${widget.currentWidth.round()}',
            increasedValue:
                '${math.min(widget.maxWidth, widget.currentWidth + 16).round()}',
            decreasedValue:
                '${math.max(widget.minWidth, widget.currentWidth - 16).round()}',
            onIncrease: () => widget.onStep(16),
            onDecrease: () => widget.onStep(-16),
            child: MouseRegion(
              cursor: SystemMouseCursors.resizeColumn,
              onEnter: (_) => setState(() => _isHovered = true),
              onExit: (_) => setState(() => _isHovered = false),
              child: Listener(
                behavior: HitTestBehavior.opaque,
                onPointerDown: (event) {
                  final now = DateTime.now();
                  if (_lastTapTime != null &&
                      now.difference(_lastTapTime!) <
                          const Duration(milliseconds: 300)) {
                    _lastTapTime = null;
                    _dragStarted = false;
                    widget.onReset();
                    return;
                  }
                  _lastTapTime = now;
                  _dragStarted = true;
                  widget.onDragStart();
                },
                onPointerMove: (event) {
                  if (_dragStarted) {
                    widget.onDragDelta(event.delta.dx);
                  }
                },
                onPointerUp: (event) {
                  if (_dragStarted) {
                    _dragStarted = false;
                    widget.onDragEnd();
                  }
                },
                onPointerCancel: (event) {
                  if (_dragStarted) {
                    _dragStarted = false;
                    widget.onDragEnd();
                  }
                },
                child: Center(
                  child: AnimatedOpacity(
                    opacity: active ? 1.0 : 0.0,
                    duration: const Duration(milliseconds: 100),
                    child: AnimatedContainer(
                      duration: const Duration(milliseconds: 120),
                      curve: Curves.easeOut,
                      width: 3.0,
                      height: active ? 40.0 : 20.0,
                      decoration: BoxDecoration(
                        color: colors.textSecondary,
                        borderRadius: BorderRadius.circular(2.0),
                      ),
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
