import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_mode.dart';

/// The platform window; the channel version at startup.
final nativeWindowProvider = Provider<NativeWindow>(
  (ref) => const NullNativeWindow(),
);

/// What the window was created with; [WindowInfo.none] until configured.
final windowInfoProvider = Provider<WindowInfo>((ref) => WindowInfo.none);

final windowChromeProvider = Provider<WindowChrome>(
  (ref) => ref.watch(windowInfoProvider).chrome,
);

class WindowStatusNotifier extends Notifier<WindowStatus> {
  @override
  WindowStatus build() {
    final subscription = ref
        .watch(nativeWindowProvider)
        .status
        .listen((status) => state = status);
    ref.onDispose(subscription.cancel);
    return ref.watch(windowInfoProvider).status;
  }
}

final windowStatusProvider =
    NotifierProvider<WindowStatusNotifier, WindowStatus>(
      WindowStatusNotifier.new,
    );

class ButtonLayoutNotifier extends Notifier<ButtonLayout> {
  @override
  ButtonLayout build() {
    final subscription = ref
        .watch(nativeWindowProvider)
        .buttonLayout
        .listen((layout) => state = layout);
    ref.onDispose(subscription.cancel);
    return ref.watch(windowInfoProvider).buttonLayout;
  }
}

final buttonLayoutProvider =
    NotifierProvider<ButtonLayoutNotifier, ButtonLayout>(
      ButtonLayoutNotifier.new,
    );

class MaximizeHoverNotifier extends Notifier<CaptionHover> {
  @override
  CaptionHover build() {
    final subscription = ref
        .watch(nativeWindowProvider)
        .maximizeHover
        .listen((hover) => state = hover);
    ref.onDispose(subscription.cancel);
    return CaptionHover.none;
  }
}

/// The Windows maximize button's hover state, from the runner.
final maximizeHoverProvider =
    NotifierProvider<MaximizeHoverNotifier, CaptionHover>(
      MaximizeHoverNotifier.new,
    );

const windowGeometryKey = 'ui.window';

/// The saved window geometry, if any.
WindowGeometry? savedWindowGeometry(KeyValueStore store) {
  final saved = store.read(windowGeometryKey);
  if (saved == null) return null;
  try {
    return WindowGeometry.fromJson(jsonDecode(saved));
  } on FormatException {
    return null;
  }
}

/// Saves the window's size, place and maximized state as they change, so
/// the next start restores them (§3.1). Saving waits for half a second of
/// quiet so dragging and resizing do not write on every frame.
class WindowGeometryKeeper {
  WindowGeometryKeeper({
    required NativeWindow window,
    required this.store,
    WindowGeometry? initial,
    this.delay = const Duration(milliseconds: 500),
  }) : _last = initial {
    _subscriptions.add(window.geometry.listen(_onGeometry));
    _subscriptions.add(window.status.listen(_onStatus));
  }

  final KeyValueStore store;
  final Duration delay;
  final _subscriptions = <StreamSubscription<Object?>>[];
  WindowGeometry? _last;
  WindowStatus _status = const WindowStatus();
  Timer? _timer;

  void _onGeometry(WindowGeometry geometry) {
    // Only a floating window's size and place are worth restoring.
    if (!_status.floating) return;
    _last = geometry.copyWith(maximized: false);
    _schedule();
  }

  void _onStatus(WindowStatus status) {
    _status = status;
    final last = _last;
    if (last == null || last.maximized == status.maximized) return;
    _last = last.copyWith(maximized: status.maximized);
    _schedule();
  }

  void _schedule() {
    _timer?.cancel();
    _timer = Timer(delay, () {
      final last = _last;
      if (last != null) {
        store.write(windowGeometryKey, jsonEncode(last.toJson()));
      }
    });
  }

  void dispose() {
    _timer?.cancel();
    for (final subscription in _subscriptions) {
      subscription.cancel();
    }
  }
}
