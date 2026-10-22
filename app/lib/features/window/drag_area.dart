import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';

/// Empty title-bar space (§3.1): dragging moves the window, a double click
/// maximizes or restores it, and a right click opens the window menu. Put
/// it behind a header's content so buttons keep their clicks.
class WindowDragArea extends ConsumerWidget {
  const WindowDragArea({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (ref.watch(windowChromeProvider) != WindowChrome.custom) {
      return const SizedBox.expand();
    }
    final window = ref.watch(nativeWindowProvider);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onPanStart: (_) => window.startDrag(),
      onDoubleTap: window.toggleMaximize,
      onSecondaryTapUp: (_) => window.showWindowMenu(),
      child: const SizedBox.expand(),
    );
  }
}
