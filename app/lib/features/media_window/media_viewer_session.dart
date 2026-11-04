import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/native_window.dart';

/// Where the viewer window's last mode and normal size are kept (D86).
const mediaViewerPlacementKey = 'ui.mediaViewer';

/// What to show: a message's photos and videos, from [index].
@immutable
class MediaViewerRequest {
  const MediaViewerRequest({
    required this.serverKey,
    required this.media,
    required this.index,
    this.senderName,
    this.sentAt,
  });

  final String serverKey;
  final List<Attachment> media;
  final int index;
  final String? senderName;
  final DateTime? sentAt;

  @override
  bool operator ==(Object other) =>
      other is MediaViewerRequest &&
      other.serverKey == serverKey &&
      listEquals(other.media, media) &&
      other.index == index &&
      other.senderName == senderName &&
      other.sentAt == sentAt;

  @override
  int get hashCode =>
      Object.hash(serverKey, Object.hashAll(media), index, senderName, sentAt);
}

/// The viewer while its window is open.
@immutable
class MediaViewerSession {
  const MediaViewerSession({
    required this.request,
    required this.viewId,
    required this.status,
    required this.generation,
  });

  final MediaViewerRequest request;

  /// The Flutter view the window shows.
  final int viewId;
  final MediaWindowStatus status;

  /// Goes up with each request, so the viewer starts again at its item.
  final int generation;

  MediaViewerSession copyWith({
    MediaViewerRequest? request,
    MediaWindowStatus? status,
    int? generation,
  }) => MediaViewerSession(
    request: request ?? this.request,
    viewId: viewId,
    status: status ?? this.status,
    generation: generation ?? this.generation,
  );
}

/// Waits until the frame after a change is drawn: a view leaves the tree
/// before the runner destroys it.
final mediaViewerFrameProvider = Provider<Future<void> Function()>(
  (ref) =>
      () => WidgetsBinding.instance.endOfFrame,
);

/// The media viewer's own window (D86): one at a time, reused for the next
/// item, closed by the app or by the system's close, its mode and normal
/// size remembered.
class MediaViewerNotifier extends Notifier<MediaViewerSession?> {
  static const _saveDelay = Duration(milliseconds: 500);

  late MediaWindowService _window;
  late KeyValueStore _store;
  MediaWindowPlacement _placement = const MediaWindowPlacement();
  Timer? _save;

  /// The window is in a mode it only passes through, not to remember.
  bool _modeHeld = false;

  /// The mode picture-in-picture goes back to.
  MediaWindowMode? _beforePip;

  /// A close the runner hasn't been told about yet. An open waits for it,
  /// or the runner would answer the window it is about to destroy.
  Future<void>? _closing;

  @override
  MediaViewerSession? build() {
    _window = ref.watch(mediaWindowServiceProvider);
    _store = ref.watch(keyValueStoreProvider);
    final subscriptions = [
      _window.closeRequests.listen((_) => close()),
      _window.status.listen(_onStatus),
      _window.geometry.listen(_onGeometry),
    ];
    ref.onDispose(() {
      _save?.cancel();
      for (final subscription in subscriptions) {
        subscription.cancel();
      }
    });
    return null;
  }

  /// Shows [request] in the window, opening it if needed. False when there
  /// is no window to open: the caller shows the viewer in the app instead.
  Future<bool> open(MediaViewerRequest request) async {
    if (_closing case final closing?) await closing;
    // Other media brings the viewer back: there is one player.
    if (state?.status.mode == MediaWindowMode.pip) await leavePip();
    final current = state;
    if (current != null) {
      state = current.copyWith(
        request: request,
        generation: current.generation + 1,
      );
      await _window.focus();
      return true;
    }
    _placement = _readPlacement();
    _modeHeld = false;
    _beforePip = null;
    final viewId = await _window.open(_placement);
    if (viewId == null) return false;
    state = MediaViewerSession(
      request: request,
      viewId: viewId,
      status: MediaWindowStatus(mode: _placement.mode),
      generation: 1,
    );
    return true;
  }

  Future<void> close() async {
    if (state == null) return;
    state = null;
    final closing = _closeWindow();
    _closing = closing;
    await closing;
    if (identical(_closing, closing)) _closing = null;
  }

  Future<void> _closeWindow() async {
    // The view goes first; destroying it under a View widget is an error.
    await ref.read(mediaViewerFrameProvider)();
    await _window.close();
  }

  /// Full screen to a normal window and back.
  Future<void> toggleMode() async {
    final current = state;
    if (current == null) return;
    await setMode(
      current.status.mode == MediaWindowMode.fullscreen
          ? MediaWindowMode.normal
          : MediaWindowMode.fullscreen,
    );
  }

  /// Puts the window in [mode]. A mode only passed through, as a video's
  /// full screen from a normal window, isn't remembered: the window opens
  /// next time as it was before.
  Future<void> setMode(MediaWindowMode mode, {bool remember = true}) async {
    final current = state;
    if (current == null) return;
    state = current.copyWith(
      status: MediaWindowStatus(mode: mode, focused: current.status.focused),
    );
    if (remember) {
      _modeHeld = false;
      _placement = _placement.copyWith(mode: mode);
      _scheduleSave();
    } else {
      _modeHeld = mode != _placement.mode;
    }
    await _window.setMode(mode);
  }

  /// Turns the window into a small one over other apps, playing on, with
  /// the video's [aspect]. Leaving goes back to the mode it had, or to
  /// [returnTo].
  Future<void> enterPip(double aspect, {MediaWindowMode? returnTo}) async {
    final current = state;
    if (current == null || current.status.mode == MediaWindowMode.pip) return;
    _beforePip = returnTo ?? current.status.mode;
    _modeHeld = true;
    state = current.copyWith(
      status: MediaWindowStatus(
        mode: MediaWindowMode.pip,
        focused: current.status.focused,
      ),
    );
    await _window.enterPip(aspect: aspect, frame: _placement.pip);
  }

  Future<void> leavePip() async {
    if (state?.status.mode != MediaWindowMode.pip) return;
    final back = _beforePip ?? _placement.mode;
    _beforePip = null;
    await setMode(back, remember: false);
  }

  void _onStatus(MediaWindowStatus status) {
    final current = state;
    if (current == null) return;
    state = current.copyWith(status: status);
    if (!_modeHeld && status.mode != _placement.mode) {
      _placement = _placement.copyWith(mode: status.mode);
      _scheduleSave();
    }
  }

  void _onGeometry(WindowGeometry geometry) {
    final status = state?.status;
    if (status == null || status.maximized) return;
    switch (status.mode) {
      case MediaWindowMode.normal:
        _placement = _placement.copyWith(normal: geometry);
      case MediaWindowMode.pip:
        _placement = _placement.copyWith(pip: geometry);
      case MediaWindowMode.fullscreen:
        return;
    }
    _scheduleSave();
  }

  void _scheduleSave() {
    _save?.cancel();
    _save = Timer(_saveDelay, () {
      _store.write(mediaViewerPlacementKey, jsonEncode(_placement.toJson()));
    });
  }

  MediaWindowPlacement _readPlacement() {
    final saved = _store.read(mediaViewerPlacementKey);
    if (saved == null) return const MediaWindowPlacement();
    try {
      return MediaWindowPlacement.fromJson(jsonDecode(saved));
    } on FormatException {
      return const MediaWindowPlacement();
    }
  }
}

final mediaViewerProvider =
    NotifierProvider<MediaViewerNotifier, MediaViewerSession?>(
      MediaViewerNotifier.new,
    );
