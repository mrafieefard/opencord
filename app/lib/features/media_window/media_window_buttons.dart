import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/media_window/media_viewer_session.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_media_colors.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// The media viewer window's title buttons (D86), drawn by the app in the
/// system's style: Windows' caption buttons at the top right, the desktop's
/// own layout on Linux. macOS keeps its real traffic lights, so nothing is
/// drawn there. Maximize switches between full screen and a normal window.
class MediaWindowButtons extends ConsumerWidget {
  const MediaWindowButtons({
    super.key,
    required this.side,
    required this.onClose,
  });

  /// Which end of the top edge this row is for: on Linux the desktop's
  /// layout splits the buttons between them.
  final ButtonSide side;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final fullscreen = ref.watch(
      mediaViewerProvider.select(
        (session) => session?.status.mode != MediaWindowMode.normal,
      ),
    );
    final window = ref.watch(mediaWindowServiceProvider);
    final viewer = ref.read(mediaViewerProvider.notifier);
    switch (Theme.of(context).platform) {
      case TargetPlatform.windows:
        if (side == ButtonSide.left) return const SizedBox.shrink();
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            _CaptionButton(
              glyph: '',
              label: 'Minimize',
              onPressed: window.minimize,
            ),
            _CaptionButton(
              glyph: fullscreen ? '' : '',
              label: fullscreen ? 'Restore' : 'Maximize',
              onPressed: viewer.toggleMode,
            ),
            _CaptionButton(
              glyph: '',
              label: 'Close',
              close: true,
              onPressed: onClose,
            ),
          ],
        );
      case TargetPlatform.linux:
        final layout = ref.watch(buttonLayoutProvider);
        final buttons = side == ButtonSide.left ? layout.left : layout.right;
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final button in buttons)
              Padding(
                padding: const EdgeInsets.all(4),
                child: GnomeWindowButton(
                  glyph: switch (button) {
                    WindowButton.minimize => WindowGlyph.minimize,
                    WindowButton.maximize =>
                      fullscreen ? WindowGlyph.restore : WindowGlyph.maximize,
                    WindowButton.close => WindowGlyph.close,
                  },
                  onPressed: switch (button) {
                    WindowButton.minimize => window.minimize,
                    WindowButton.maximize => viewer.toggleMode,
                    WindowButton.close => onClose,
                  },
                ),
              ),
          ],
        );
      default:
        return const SizedBox.shrink();
    }
  }
}

/// A Windows caption button, 46 × 32, white for the viewer's dark
/// background with a soft shadow for bright photos; close hovers red, as
/// Windows' own does.
class _CaptionButton extends StatelessWidget {
  const _CaptionButton({
    required this.glyph,
    required this.label,
    required this.onPressed,
    this.close = false,
  });

  final String glyph;
  final String label;
  final bool close;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return Hoverable(
      onTap: onPressed,
      semanticLabel: label,
      cursor: SystemMouseCursors.basic,
      focusRadius: BorderRadius.zero,
      builder: (context, state) {
        final background = switch ((state.pressed, state.hovered)) {
          (true, _) =>
            close
                ? OcMediaColors.captionClosePress
                : OcMediaColors.captionPress,
          (_, true) =>
            close
                ? OcMediaColors.captionCloseHover
                : OcMediaColors.captionHover,
          _ => null,
        };
        return Container(
          width: 46,
          height: 32,
          color: background,
          alignment: Alignment.center,
          child: ExcludeSemantics(
            child: Text(
              glyph,
              style: const TextStyle(
                fontFamily: 'Segoe Fluent Icons',
                fontFamilyFallback: ['Segoe MDL2 Assets'],
                fontSize: 10,
                color: OcMediaColors.captionGlyph,
                shadows: [
                  Shadow(color: OcMediaColors.captionShadow, blurRadius: 4),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}
