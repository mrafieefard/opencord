import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/window/window_mode.dart';

WindowChrome linux(
  String desktop, [
  WindowFramePreference preference = WindowFramePreference.auto,
]) => resolveWindowChrome(
  platform: TargetPlatform.linux,
  environment: {'XDG_CURRENT_DESKTOP': desktop, 'XDG_SESSION_TYPE': 'wayland'},
  preference: preference,
);

void main() {
  group('Auto on Linux follows the desktop (§3.1)', () {
    test('floating desktops get the custom header bar', () {
      for (final desktop in [
        'GNOME',
        'ubuntu:GNOME',
        'Budgie:GNOME',
        'X-Cinnamon',
        'Pantheon',
        'XFCE',
        'MATE',
        'Unity',
        '',
      ]) {
        expect(linux(desktop), WindowChrome.custom, reason: desktop);
      }
    });

    test('KDE Plasma keeps its own decorations', () {
      expect(linux('KDE'), WindowChrome.system);
    });

    test('tiling compositors get no controls and no decorations', () {
      for (final desktop in [
        'Hyprland',
        'sway',
        'i3',
        'river',
        'niri',
        'bspwm',
        'dwm',
      ]) {
        expect(linux(desktop), WindowChrome.bare, reason: desktop);
      }
    });
  });

  test('an explicit choice wins over Auto', () {
    expect(
      linux('Hyprland', WindowFramePreference.custom),
      WindowChrome.custom,
    );
    expect(linux('GNOME', WindowFramePreference.system), WindowChrome.system);
  });

  test('Windows and macOS draw their own chrome by default', () {
    for (final platform in [TargetPlatform.windows, TargetPlatform.macOS]) {
      expect(
        resolveWindowChrome(
          platform: platform,
          environment: const {},
          preference: WindowFramePreference.auto,
        ),
        WindowChrome.custom,
      );
      expect(
        resolveWindowChrome(
          platform: platform,
          environment: const {},
          preference: WindowFramePreference.system,
        ),
        WindowChrome.system,
      );
    }
  });

  group('the GNOME button layout', () {
    test('is read from gtk-decoration-layout', () {
      expect(
        ButtonLayout.parse('appmenu:close'),
        const ButtonLayout(left: [], right: [WindowButton.close]),
      );
      expect(
        ButtonLayout.parse(':minimize,maximize,close'),
        const ButtonLayout(
          left: [],
          right: [
            WindowButton.minimize,
            WindowButton.maximize,
            WindowButton.close,
          ],
        ),
      );
      expect(
        ButtonLayout.parse('close,minimize,maximize:'),
        const ButtonLayout(
          left: [
            WindowButton.close,
            WindowButton.minimize,
            WindowButton.maximize,
          ],
          right: [],
        ),
      );
      expect(
        ButtonLayout.parse('icon:spacer,close'),
        const ButtonLayout(left: [], right: [WindowButton.close]),
      );
    });

    test('without a colon everything is on the left, as in GTK', () {
      expect(
        ButtonLayout.parse('close'),
        const ButtonLayout(left: [WindowButton.close], right: []),
      );
    });

    test('defaults to GNOME’s close-only layout', () {
      expect(ButtonLayout.gnomeDefault.right, [WindowButton.close]);
    });
  });
}
