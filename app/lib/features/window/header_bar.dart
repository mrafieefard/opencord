import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/window/drag_area.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// A 60 px header that doubles as the window's title bar (§3.1): its empty
/// space drags the window, and it hosts the window controls in the
/// platform's style.
class HeaderBar extends ConsumerWidget {
  const HeaderBar({
    super.key,
    required this.child,
    this.leadingControls = false,
    this.trailingControls = false,
    this.padding = const EdgeInsets.symmetric(horizontal: OcSpace.s12),
  });

  final Widget child;

  /// Window controls (§3.1): the rightmost header carries the right-hand
  /// ones, and the first header the left-hand ones of a GNOME layout that
  /// puts them on the left.
  final bool leadingControls;
  final bool trailingControls;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final custom = ref.watch(windowChromeProvider) == WindowChrome.custom;
    final fullscreen = ref.watch(
      windowStatusProvider.select((status) => status.fullscreen),
    );
    final platform = Theme.of(context).platform;
    final controls = custom && !fullscreen;
    final windows = platform == TargetPlatform.windows;
    final linux = platform == TargetPlatform.linux;
    final leading = controls && linux && leadingControls;
    final trailing = controls && linux && trailingControls;
    final caption = controls && windows && trailingControls;
    return Container(
      height: OcSize.header,
      decoration: BoxDecoration(
        color: colors.sidebar,
        border: Border(bottom: BorderSide(color: colors.border)),
      ),
      child: Stack(
        children: [
          const Positioned.fill(child: WindowDragArea()),
          Positioned.fill(
            child: Padding(
              padding: padding.copyWith(
                right: caption ? WindowsCaptionButtons.reserved : null,
              ),
              child: Row(
                children: [
                  if (leading) ...[
                    const LinuxWindowButtons(side: ButtonSide.left),
                    const SizedBox(width: OcSpace.s4),
                  ],
                  Expanded(child: child),
                  if (trailing) ...[
                    const SizedBox(width: OcSpace.s4),
                    const LinuxWindowButtons(side: ButtonSide.right),
                  ],
                ],
              ),
            ),
          ),
          if (caption)
            const Positioned(top: 0, right: 0, child: WindowsCaptionButtons()),
        ],
      ),
    );
  }
}

/// Whether a header asked for the right-hand window controls actually shows
/// any: the custom frame is on, the window is not fullscreen, and the
/// platform puts controls on the right.
bool showsTrailingWindowControls(BuildContext context, WidgetRef ref) {
  if (ref.watch(windowChromeProvider) != WindowChrome.custom) return false;
  if (ref.watch(windowStatusProvider.select((s) => s.fullscreen))) {
    return false;
  }
  return switch (Theme.of(context).platform) {
    TargetPlatform.windows => true,
    TargetPlatform.linux => ref.watch(
      buttonLayoutProvider.select((layout) => layout.right.isNotEmpty),
    ),
    _ => false,
  };
}
