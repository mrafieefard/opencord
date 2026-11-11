import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';

/// Shows a popup menu clamped within the plugin's [Overlay] area.
OverlayEntry? showBlocksPopup({
  required BuildContext context,
  required BlocksTheme theme,
  required List<Widget> Function(VoidCallback close) itemsBuilder,
  double minWidth = 140,
}) {
  final overlay = Overlay.of(context);
  final overlayBox = overlay.context.findRenderObject() as RenderBox?;
  final triggerBox = context.findRenderObject() as RenderBox?;
  if (overlayBox == null || triggerBox == null) return null;

  final triggerOffset = triggerBox.localToGlobal(
    Offset.zero,
    ancestor: overlayBox,
  );
  final triggerSize = triggerBox.size;
  final overlaySize = overlayBox.size;

  late OverlayEntry entry;
  void close() {
    if (entry.mounted) {
      entry.remove();
    }
  }

  entry = OverlayEntry(
    builder: (context) {
      final items = itemsBuilder(close);
      return CallbackShortcuts(
        bindings: {const SingleActivator(LogicalKeyboardKey.escape): close},
        child: FocusScope(
          autofocus: true,
          child: Stack(
            children: [
              Positioned.fill(
                child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTap: close,
                  child: const ColoredBox(color: Colors.transparent),
                ),
              ),
              CustomSingleChildLayout(
                delegate: _BlocksPopupLayoutDelegate(
                  triggerOffset: triggerOffset,
                  triggerSize: triggerSize,
                  overlaySize: overlaySize,
                  minWidth: minWidth,
                ),
                child: Material(
                  type: MaterialType.transparency,
                  child: Container(
                    decoration: BoxDecoration(
                      color: theme.colors['elevated']!,
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(
                        color: theme.colors['border']!,
                        width: 1,
                      ),
                      boxShadow: const [
                        BoxShadow(
                          color: Color(0x33000000),
                          blurRadius: 8,
                          offset: Offset(0, 4),
                        ),
                      ],
                    ),
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(8),
                      child: SingleChildScrollView(
                        child: IntrinsicWidth(
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: items,
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      );
    },
  );

  overlay.insert(entry);
  return entry;
}

class _BlocksPopupLayoutDelegate extends SingleChildLayoutDelegate {
  _BlocksPopupLayoutDelegate({
    required this.triggerOffset,
    required this.triggerSize,
    required this.overlaySize,
    required this.minWidth,
  });

  final Offset triggerOffset;
  final Size triggerSize;
  final Size overlaySize;
  final double minWidth;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    final double maxW = (overlaySize.width - 16).clamp(0.0, double.infinity);
    final double maxH = (overlaySize.height - 16).clamp(0.0, double.infinity);
    final double minW = math.min(math.max(minWidth, triggerSize.width), maxW);
    return BoxConstraints(minWidth: minW, maxWidth: maxW, maxHeight: maxH);
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final spaceBelow =
        overlaySize.height - (triggerOffset.dy + triggerSize.height) - 8;
    final spaceAbove = triggerOffset.dy - 8;

    double y;
    if (childSize.height <= spaceBelow || spaceBelow >= spaceAbove) {
      y = triggerOffset.dy + triggerSize.height + 4;
      if (y + childSize.height > overlaySize.height - 8) {
        y = math.max(8.0, overlaySize.height - 8 - childSize.height);
      }
    } else {
      y = triggerOffset.dy - childSize.height - 4;
      if (y < 8) {
        y = 8;
      }
    }

    double x = triggerOffset.dx;
    if (x + childSize.width > overlaySize.width - 8) {
      x = math.max(8.0, overlaySize.width - 8 - childSize.width);
    }
    if (x < 8) {
      x = 8;
    }

    return Offset(x, y);
  }

  @override
  bool shouldRelayout(_BlocksPopupLayoutDelegate oldDelegate) {
    return oldDelegate.triggerOffset != triggerOffset ||
        oldDelegate.triggerSize != triggerSize ||
        oldDelegate.overlaySize != overlaySize;
  }
}
