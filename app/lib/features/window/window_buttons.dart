import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

enum ButtonSide { left, right }

/// GNOME-style window buttons (§3.1): round 24 px buttons in the header,
/// in the order and on the side the user's button layout says.
class LinuxWindowButtons extends ConsumerWidget {
  const LinuxWindowButtons({
    super.key,
    required this.side,
    this.extent = defaultExtent,
  });

  final ButtonSide side;
  final double extent;

  /// The hit area of one button; the round button itself is 24 px.
  static const double defaultExtent = 36;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final layout = ref.watch(buttonLayoutProvider);
    final buttons = side == ButtonSide.left ? layout.left : layout.right;
    if (buttons.isEmpty) return const SizedBox.shrink();
    final maximized = ref.watch(
      windowStatusProvider.select((status) => status.maximized),
    );
    final window = ref.watch(nativeWindowProvider);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final button in buttons)
          GnomeWindowButton(
            extent: extent,
            glyph: switch (button) {
              WindowButton.minimize => WindowGlyph.minimize,
              WindowButton.maximize =>
                maximized ? WindowGlyph.restore : WindowGlyph.maximize,
              WindowButton.close => WindowGlyph.close,
            },
            onPressed: switch (button) {
              WindowButton.minimize => window.minimize,
              WindowButton.maximize => window.toggleMaximize,
              WindowButton.close => window.close,
            },
          ),
      ],
    );
  }
}

/// Minimize and maximize for the window, and a close that calls [onClose]
/// instead of closing it: for a layer over the whole window that hides the
/// header's controls (the media viewer). Minimize and maximize show only
/// where the app draws the window's frame, as the header's do.
class OverlayWindowButtons extends ConsumerWidget {
  const OverlayWindowButtons({super.key, required this.onClose});

  final VoidCallback onClose;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final custom = ref.watch(windowChromeProvider) == WindowChrome.custom;
    final status = ref.watch(windowStatusProvider);
    final platform = Theme.of(context).platform;
    final window = ref.watch(nativeWindowProvider);
    final frameButtons =
        custom &&
        !status.fullscreen &&
        (platform == TargetPlatform.linux ||
            platform == TargetPlatform.windows);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (frameButtons) ...[
          GnomeWindowButton(
            glyph: WindowGlyph.minimize,
            onPressed: window.minimize,
          ),
          GnomeWindowButton(
            glyph: status.maximized
                ? WindowGlyph.restore
                : WindowGlyph.maximize,
            onPressed: window.toggleMaximize,
          ),
        ],
        GnomeWindowButton(glyph: WindowGlyph.close, onPressed: onClose),
      ],
    );
  }
}

/// The window buttons' glyphs, and what they say to a screen reader.
enum WindowGlyph {
  minimize('Minimize'),
  maximize('Maximize'),
  restore('Restore'),
  close('Close');

  const WindowGlyph(this.label);

  final String label;
}

/// One GNOME-style round window button (§3.1).
class GnomeWindowButton extends StatelessWidget {
  const GnomeWindowButton({
    super.key,
    required this.glyph,
    required this.onPressed,
    this.extent = LinuxWindowButtons.defaultExtent,
  });

  final WindowGlyph glyph;
  final VoidCallback onPressed;
  final double extent;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Tooltip(
      message: glyph.label,
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: onPressed,
        semanticLabel: glyph.label,
        focusRadius: BorderRadius.circular(12),
        builder: (context, state) {
          final background = state.pressed
              ? colors.accent
              : state.hovered
              ? colors.selected
              : colors.hover;
          return SizedBox.square(
            dimension: extent,
            child: Center(
              child: AnimatedContainer(
                duration: OcMotion.of(context).hover,
                width: 24,
                height: 24,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: background,
                ),
                child: CustomPaint(
                  painter: _GlyphPainter(
                    glyph,
                    state.pressed ? colors.onAccent : colors.text,
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}

/// GNOME's symbolic window icons, drawn at 16 px inside the 24 px button.
class _GlyphPainter extends CustomPainter {
  const _GlyphPainter(this.glyph, this.color);

  final WindowGlyph glyph;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = 1.2
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round;
    final c = size.center(Offset.zero);
    const half = 4.5;
    switch (glyph) {
      case WindowGlyph.minimize:
        canvas.drawLine(
          c + const Offset(-half, half - 1),
          c + const Offset(half, half - 1),
          paint,
        );
      case WindowGlyph.maximize:
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            Rect.fromCenter(center: c, width: half * 2, height: half * 2),
            const Radius.circular(1.5),
          ),
          paint,
        );
      case WindowGlyph.restore:
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            Rect.fromLTWH(c.dx - half, c.dy - half + 2.5, 6.5, 6.5),
            const Radius.circular(1.2),
          ),
          paint,
        );
        canvas.drawPath(
          Path()
            ..moveTo(c.dx - half + 2.5, c.dy - half + 0.5)
            ..lineTo(c.dx - half + 2.5, c.dy - half)
            ..lineTo(c.dx + half, c.dy - half)
            ..lineTo(c.dx + half, c.dy + half - 2.5)
            ..lineTo(c.dx + half - 0.5, c.dy + half - 2.5),
          paint,
        );
      case WindowGlyph.close:
        final d = half * math.cos(math.pi / 4) * 1.3;
        canvas.drawLine(c + Offset(-d, -d), c + Offset(d, d), paint);
        canvas.drawLine(c + Offset(d, -d), c + Offset(-d, d), paint);
    }
  }

  @override
  bool shouldRepaint(_GlyphPainter old) =>
      old.glyph != glyph || old.color != color;
}

/// Windows 10/11 caption buttons (§3.1): 46 × 32 px, flush with the top
/// right corner, Segoe Fluent Icons glyphs. Closing hovers inverted instead
/// of red, to keep the app monochrome.
class WindowsCaptionButtons extends ConsumerWidget {
  const WindowsCaptionButtons({super.key});

  /// Space a header keeps free for the buttons.
  static const double reserved = 138;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final status = ref.watch(windowStatusProvider);
    final window = ref.watch(nativeWindowProvider);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _CaptionButton(
          glyph: '\uE921',
          label: 'Minimize',
          focused: status.focused,
          onPressed: window.minimize,
        ),
        _CaptionButton(
          glyph: status.maximized ? '\uE923' : '\uE922',
          label: status.maximized ? 'Restore' : 'Maximize',
          focused: status.focused,
          native: ref.watch(maximizeHoverProvider),
          onPressed: window.toggleMaximize,
        ),
        _CaptionButton(
          glyph: '\uE8BB',
          label: 'Close',
          focused: status.focused,
          close: true,
          onPressed: window.close,
        ),
      ],
    );
  }
}

class _CaptionButton extends StatelessWidget {
  const _CaptionButton({
    required this.glyph,
    required this.label,
    required this.focused,
    required this.onPressed,
    this.close = false,
    this.native = CaptionHover.none,
  });

  final String glyph;
  final String label;
  final bool focused;
  final bool close;
  final VoidCallback onPressed;

  /// Hover and press as the runner reports them (the maximize button).
  final CaptionHover native;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onPressed,
      semanticLabel: label,
      cursor: SystemMouseCursors.basic,
      focusRadius: BorderRadius.zero,
      builder: (context, state) {
        final pressed = state.pressed || native == CaptionHover.pressed;
        final hovered = state.hovered || native == CaptionHover.hover;
        final inverted = close && (pressed || hovered);
        final background = inverted
            ? colors.accent
            : pressed
            ? colors.selected
            : hovered
            ? colors.hover
            : null;
        return Container(
          width: 46,
          height: 32,
          color: background,
          alignment: Alignment.center,
          child: ExcludeSemantics(
            child: Text(
              glyph,
              style: TextStyle(
                fontFamily: 'Segoe Fluent Icons',
                fontFamilyFallback: const ['Segoe MDL2 Assets'],
                fontSize: 10,
                color: inverted
                    ? colors.onAccent
                    : focused
                    ? colors.text
                    : colors.textSecondary,
              ),
            ),
          ),
        );
      },
    );
  }
}
