import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/window/drag_area.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// The one row fixed to the window's top (floating UI §5.1): flush, on the
/// ground's colour, no border. Its empty space drags the window, and it
/// hosts the window controls in the platform's style. In fullscreen it
/// stays, without them.
class AppTitleBar extends ConsumerWidget {
  const AppTitleBar({super.key, this.leading, this.center});

  /// At the row's start, after any left-hand window buttons: the arrows.
  final Widget? leading;

  /// At the window's true centre, whatever the sides hold: the crumb. It
  /// ignores the pointer, so the row drags through it.
  final Widget? center;

  /// A Linux button's hit area here: its 24 px circle then sits 6 from the
  /// next one.
  static const double _linuxButtonExtent = 30;

  /// The row's padding beside Linux buttons, which with their hit areas puts
  /// the outermost circle `OcFloats.gap` from the window's edge.
  static const double _linuxButtonInset = 5;

  /// The room for macOS's traffic lights: `bandWidth` in
  /// `MainFlutterWindow.swift`.
  static const double _lightsWidth = 78;

  static const double _centerWidthFactor = 0.56;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final custom = ref.watch(windowChromeProvider) == WindowChrome.custom;
    final fullscreen = ref.watch(
      windowStatusProvider.select((status) => status.fullscreen),
    );
    final layout = ref.watch(buttonLayoutProvider);
    final platform = Theme.of(context).platform;
    final controls = custom && !fullscreen;
    final linux = controls && platform == TargetPlatform.linux;
    final caption = controls && platform == TargetPlatform.windows;
    final lights = platform == TargetPlatform.macOS && !fullscreen;
    final left = lights
        ? _lightsWidth
        : linux && layout.left.isNotEmpty
        ? _linuxButtonInset
        : OcFloats.gap;
    final right = linux && layout.right.isNotEmpty
        ? _linuxButtonInset
        : OcFloats.gap;
    return ColoredBox(
      key: ShellKeys.titlebar,
      color: context.oc.rail,
      child: SizedBox(
        height: OcFloats.titleBar,
        child: Stack(
          children: [
            const Positioned.fill(child: WindowDragArea()),
            if (center != null)
              Positioned.fill(
                child: IgnorePointer(
                  child: FractionallySizedBox(
                    widthFactor: _centerWidthFactor,
                    child: Center(child: center),
                  ),
                ),
              ),
            Positioned.fill(
              child: Padding(
                padding: EdgeInsets.only(left: left, right: right),
                child: Row(
                  children: [
                    if (linux)
                      const LinuxWindowButtons(
                        side: ButtonSide.left,
                        extent: _linuxButtonExtent,
                      ),
                    Expanded(
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: leading,
                      ),
                    ),
                    if (linux)
                      const LinuxWindowButtons(
                        side: ButtonSide.right,
                        extent: _linuxButtonExtent,
                      ),
                  ],
                ),
              ),
            ),
            if (caption)
              const Positioned(
                top: 0,
                right: 0,
                child: WindowsCaptionButtons(),
              ),
          ],
        ),
      ),
    );
  }
}
