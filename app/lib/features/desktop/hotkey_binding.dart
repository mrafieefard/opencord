import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/window/window_providers.dart';

/// Binds the hotkeys again each time the window is focused while they wait
/// for the user to allow them in the system's settings (macOS's Input
/// Monitoring), so a grant takes effect without a restart.
final hotkeyBindingProvider = Provider<void>((ref) {
  ref.listen(windowStatusProvider.select((status) => status.focused), (
    before,
    focused,
  ) {
    final waiting = switch (ref.read(hotkeySupportProvider)) {
      HotkeysFocusedOnly(:final settingsUrl) => settingsUrl != null,
      _ => false,
    };
    if (focused && before == false && waiting) rebindHotkeys(ref);
  });
});
