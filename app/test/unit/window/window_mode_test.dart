import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/window/window_mode.dart';

void main() {
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
