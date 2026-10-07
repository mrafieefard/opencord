import 'dart:convert';
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/physics.dart';
import 'package:flutter/widgets.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';

/// Where a chat opens (§6).
sealed class ChatStart {
  const ChatStart();
}

/// At the newest message.
final class StartAtEnd extends ChatStart {
  const StartAtEnd();
}

/// With the anchor line (the top of the newer half) [fromTop] pixels plus
/// [fraction] of the view's height below its top: the unread line, or a
/// remembered position.
final class StartAtAnchor extends ChatStart {
  const StartAtAnchor(this.fromTop, {this.fraction = 0});

  final double fromTop;
  final double fraction;
}

/// Where the reader left a channel, so coming back puts them there again
/// (§16). Kept for this run of the app only.
@immutable
class SavedScroll {
  const SavedScroll({required this.messageId, required this.fromTop});

  /// The topmost message in view, and how far below the top it started.
  final int messageId;
  final double fromTop;

  @override
  bool operator ==(Object other) =>
      other is SavedScroll &&
      other.messageId == messageId &&
      other.fromTop == fromTop;

  @override
  int get hashCode => Object.hash(messageId, fromTop);
}

/// Where each channel was left (§16 "remember everything"): in memory while
/// the app runs, and in [KeyValueStore] shortly after the reader stops
/// scrolling, so it survives a restart. Keeps the [limit] most recent.
class ScrollMemory {
  ScrollMemory([this._store]) {
    _load();
  }

  static const storeKey = 'ui.scroll';
  static const limit = 200;
  static const _settle = Duration(milliseconds: 1500);

  final KeyValueStore? _store;

  /// By `server|channel`, oldest first.
  final _saved = <String, SavedScroll>{};
  Timer? _flush;

  static String _key(ChannelRef channel) =>
      '${channel.server}|${channel.channel}';

  /// [position] null: the reader is at the newest message.
  void save(ChannelRef channel, SavedScroll? position) {
    final key = _key(channel);
    final previous = _saved.remove(key);
    if (position != null) _saved[key] = position;
    while (_saved.length > limit) {
      _saved.remove(_saved.keys.first);
    }
    if (_store == null || previous == position) return;
    _flush?.cancel();
    _flush = Timer(_settle, flush);
  }

  SavedScroll? read(ChannelRef channel) => _saved[_key(channel)];

  /// Writes what is remembered now.
  void flush() {
    _flush?.cancel();
    _flush = null;
    _store?.write(
      storeKey,
      jsonEncode({
        for (final MapEntry(:key, :value) in _saved.entries)
          key: [value.messageId, value.fromTop],
      }),
    );
  }

  void _load() {
    final text = _store?.read(storeKey);
    if (text == null) return;
    final Object? json;
    try {
      json = jsonDecode(text);
    } on FormatException {
      return;
    }
    if (json is! Map) return;
    for (final MapEntry(:key, :value) in json.entries) {
      if (value case [final int id, final num fromTop] when key is String) {
        _saved[key] = SavedScroll(messageId: id, fromTop: fromTop.toDouble());
      }
    }
  }
}

/// Scrolls a chat built as two slivers around an anchor (older history
/// growing up, newer messages growing down, the anchor at the bottom of the
/// view): loading history above or messages below never moves what is on
/// screen, and while the reader is at the newest message the view follows
/// new ones (§6).
class ChatScrollController extends ScrollController {
  ChatScrollController({
    this.start = const StartAtEnd(),
    this.wheel = Duration.zero,
  });

  final ChatStart start;

  /// How long a mouse wheel notch glides; zero jumps.
  final Duration wheel;

  @override
  ChatScrollPosition get position => super.position as ChatScrollPosition;

  @override
  ScrollPosition createScrollPosition(
    ScrollPhysics physics,
    ScrollContext context,
    ScrollPosition? oldPosition,
  ) => ChatScrollPosition(
    physics: physics,
    context: context,
    oldPosition: oldPosition,
    start: start,
    wheel: wheel,
  );

  /// Whether the newest message is in view.
  bool get atEnd => hasClients && position.atEnd;

  /// Jumps to the newest message, or glides there when it is close.
  Future<void> scrollToEnd({required Duration duration}) async {
    if (!hasClients) return;
    final position = this.position;
    final distance = position.maxScrollExtent - position.pixels;
    if (duration == Duration.zero ||
        distance > position.viewportDimension * 3) {
      position.jumpTo(position.maxScrollExtent);
      return;
    }
    await position.animateTo(
      position.maxScrollExtent,
      duration: duration,
      curve: Curves.easeOut,
    );
    // Messages that arrived during the glide are followed from here on.
    if (hasClients) position.jumpTo(position.maxScrollExtent);
  }
}

class ChatScrollPosition extends ScrollPositionWithSingleContext {
  ChatScrollPosition({
    required super.physics,
    required super.context,
    super.oldPosition,
    required this.start,
    this.wheel = Duration.zero,
  });

  final ChatStart start;
  final Duration wheel;

  /// Steps this small come from touchpads and follow the fingers at once;
  /// mouse wheel notches are larger.
  static const double _notch = 20;

  /// Where the notches so far are heading, while they glide.
  double? _wheelTarget;

  /// Within this distance of the newest message counts as being there.
  static const double endSlop = 2;

  bool get atEnd =>
      hasContentDimensions && hasPixels && pixels >= maxScrollExtent - endSlop;

  /// Wheel notches glide there instead of jumping (§15 smooth wheel
  /// scrolling), and quick ones add up.
  @override
  void pointerScroll(double delta) {
    if (wheel == Duration.zero || delta.abs() < _notch) {
      _wheelTarget = null;
      super.pointerScroll(delta);
      return;
    }
    final target = ((_wheelTarget ?? pixels) + delta).clamp(
      minScrollExtent,
      maxScrollExtent,
    );
    if (target == pixels) return;
    _wheelTarget = target;
    animateTo(target, duration: wheel, curve: Curves.easeOutCubic).whenComplete(
      () {
        if (_wheelTarget == target) _wheelTarget = null;
      },
    );
  }

  @override
  bool applyContentDimensions(double minScrollExtent, double maxScrollExtent) {
    final first = !hasContentDimensions;
    final follow = first ? start is StartAtEnd : atEnd;
    final settled = super.applyContentDimensions(
      minScrollExtent,
      maxScrollExtent,
    );
    final double target;
    if (follow) {
      target = maxScrollExtent;
    } else if (start case StartAtAnchor(
      :final fromTop,
      :final fraction,
    ) when first) {
      target = (viewportDimension * (1 - fraction) - fromTop).clamp(
        minScrollExtent,
        math.max(minScrollExtent, maxScrollExtent),
      );
    } else {
      return settled;
    }
    if (nearEqual(pixels, target, Tolerance.defaultTolerance.distance)) {
      return settled;
    }
    correctPixels(target);
    return false;
  }
}
