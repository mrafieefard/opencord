import 'dart:math' as math;

import 'package:flutter/physics.dart';
import 'package:flutter/widgets.dart';

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
}

class ScrollMemory {
  final _saved = <Object, SavedScroll>{};

  void save(Object channel, SavedScroll? position) {
    if (position == null) {
      _saved.remove(channel);
    } else {
      _saved[channel] = position;
    }
  }

  SavedScroll? read(Object channel) => _saved[channel];
}

/// Scrolls a chat built as two slivers around an anchor (older history
/// growing up, newer messages growing down, the anchor at the bottom of the
/// view): loading history above or messages below never moves what is on
/// screen, and while the reader is at the newest message the view follows
/// new ones (§6).
class ChatScrollController extends ScrollController {
  ChatScrollController({this.start = const StartAtEnd()});

  final ChatStart start;

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
  });

  final ChatStart start;

  /// Within this distance of the newest message counts as being there.
  static const double endSlop = 2;

  bool get atEnd =>
      hasContentDimensions && hasPixels && pixels >= maxScrollExtent - endSlop;

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
