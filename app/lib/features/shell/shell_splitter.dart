import 'dart:math' as math;

import 'package:clock/clock.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

/// The key under which the user's custom channel list width is persisted.
const sidebarWidthKey = 'ui.sidebarWidth';

/// A width the user dragged a column to, persisted under [storageKey].
/// `null` means the column's default.
abstract class StoredWidthNotifier extends Notifier<double?> {
  String get storageKey;

  @override
  double? build() {
    final store = ref.watch(keyValueStoreProvider);
    final raw = store.read(storageKey);
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
      store.write(storageKey, null);
    } else {
      store.write(storageKey, width.toString());
    }
  }

  /// Resets to default (null) and removes the key from storage.
  void reset() => save(null);
}

/// Holds the saved or active width of the left column (§5.7).
///
/// `null` means the layout class's default (288 px for wide/medium, 264 px for
/// narrow).
class SidebarWidthNotifier extends StoredWidthNotifier {
  static const key = sidebarWidthKey;

  @override
  String get storageKey => key;
}

/// The provider for the user's channel list width.
final sidebarWidthProvider = NotifierProvider<SidebarWidthNotifier, double?>(
  SidebarWidthNotifier.new,
);

/// What the right pane holds while it's inline (follow-ups §2): each keeps
/// its own width.
enum SideContent {
  people(OcFloats.side, OcFloats.sideMin, 'ui.sideWidth.people'),
  music(OcFloats.sideMusic, OcFloats.sideMusicMin, 'ui.sideWidth.music'),
  voiceChat(
    OcFloats.voiceChat,
    OcFloats.voiceChatMin,
    'ui.sideWidth.voiceChat',
  );

  const SideContent(this.defaultWidth, this.minWidth, this.key);

  final double defaultWidth;
  final double minWidth;

  /// Where its width is persisted.
  final String key;

  /// The room main keeps beside it: a voice channel's chat leaves the stage
  /// its minimum.
  double get mainMin =>
      this == voiceChat ? OcFloats.voiceStageMin : OcFloats.mainMin;
}

/// Holds the saved or active width of the right pane for one [content].
class SideWidthNotifier extends StoredWidthNotifier {
  SideWidthNotifier(this.content);

  final SideContent content;

  @override
  String get storageKey => content.key;
}

/// The right pane's width for each content, `null` for its default.
final sideWidthProvider =
    NotifierProvider.family<SideWidthNotifier, double?, SideContent>(
      SideWidthNotifier.new,
    );

/// A draggable splitter beside a column (§5.7): the left column's, or the
/// right pane's (follow-ups §2).
///
/// Sits over the 8 px gap, on top of the floats either side, so its hit area
/// is the gap and 4 on each side: [hitWidth]. [gapKey] is the gap. Shows a
/// 3 × 40 pill while hovered, focused or dragging.
///
/// Its callbacks speak in the column's width: with [growsLeft], moving the
/// edge left (by pointer or ←) widens it.
class ShellSplitter extends StatefulWidget {
  const ShellSplitter({
    super.key,
    required this.label,
    this.gapKey,
    this.growsLeft = false,
    required this.currentWidth,
    required this.minWidth,
    required this.maxWidth,
    required this.isDragging,
    required this.onDragStart,
    required this.onDragDelta,
    required this.onDragEnd,
    required this.onReset,
    required this.onStep,
    this.focusNode,
  });

  /// What a screen reader calls the slider.
  final String label;
  final Key? gapKey;
  final bool growsLeft;
  final double currentWidth;
  final double minWidth;
  final double maxWidth;
  final bool isDragging;
  final VoidCallback onDragStart;
  final ValueChanged<double> onDragDelta;
  final VoidCallback onDragEnd;
  final VoidCallback onReset;
  final ValueChanged<double> onStep;
  final FocusNode? focusNode;

  static const double hitWidth = OcFloats.gap + 8;

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

  /// The width's change for the edge moving [dx] to the right.
  double _widthChange(double dx) => widget.growsLeft ? -dx : dx;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final active = _isHovered || _isFocused || widget.isDragging;
    final step = _widthChange(16);

    return SizedBox(
      width: ShellSplitter.hitWidth,
      child: MouseRegion(
        cursor: SystemMouseCursors.resizeColumn,
        onEnter: (_) => setState(() => _isHovered = true),
        onExit: (_) => setState(() => _isHovered = false),
        child: Listener(
          behavior: HitTestBehavior.opaque,
          onPointerDown: (event) {
            if (event.buttons != kPrimaryButton) return;
            final now = clock.now();
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
              // A drag isn't half of a double click.
              _lastTapTime = null;
              widget.onDragDelta(_widthChange(event.delta.dx));
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
            child: SizedBox(
              key: widget.gapKey,
              width: OcFloats.gap,
              height: double.infinity,
              child: Focus(
                focusNode: _effectiveFocusNode,
                onKeyEvent: (node, event) {
                  if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
                    return KeyEventResult.ignored;
                  }
                  if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
                    widget.onStep(-step);
                    return KeyEventResult.handled;
                  }
                  if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
                    widget.onStep(step);
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
                  label: widget.label,
                  value: '${widget.currentWidth.round()}',
                  increasedValue:
                      '${math.min(widget.maxWidth, widget.currentWidth + 16).round()}',
                  decreasedValue:
                      '${math.max(widget.minWidth, widget.currentWidth - 16).round()}',
                  onIncrease: () => widget.onStep(16),
                  onDecrease: () => widget.onStep(-16),
                  child: Center(
                    child: AnimatedOpacity(
                      opacity: active ? 1.0 : 0.0,
                      duration: motion.fadeQuick,
                      curve: OcMotion.curve,
                      child: AnimatedContainer(
                        duration: motion.hover,
                        curve: OcMotion.curve,
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
      ),
    );
  }
}
