import 'dart:async';

import 'package:flutter/widgets.dart';

import '../view.dart';

/// `scroll` (design §12.2): vertical or horizontal scrolling with `scroll`,
/// `reach_start` and `reach_end` events.
final scrollBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final children = b.node.children;
  if (children.isEmpty) return const SizedBox.shrink();
  final childId = children.first;

  return _ScrollArea(
    build: b,
    child: BlockWidget(key: ValueKey<int>(childId), id: childId),
  );
}

/// Throttles scroll events to at most 10 per second (100 ms) and formats
/// the event payload as `{'offset': pixels, 'extent': maxScrollExtent}`.
class BlocksScrollThrottle {
  DateTime _lastScrollSent = DateTime.fromMillisecondsSinceEpoch(0);
  Timer? _pendingScrollTimer;
  Map<String, Object?>? _pendingScrollData;

  void handleScroll({
    required ScrollPosition position,
    required bool handlesScroll,
    required void Function(Map<String, Object?> data) sendScroll,
  }) {
    if (!handlesScroll) return;
    final data = {
      'offset': position.pixels,
      'extent': position.maxScrollExtent,
    };
    final now = DateTime.now();
    final elapsed = now.difference(_lastScrollSent).inMilliseconds;
    if (elapsed >= 100) {
      _lastScrollSent = now;
      _pendingScrollTimer?.cancel();
      _pendingScrollTimer = null;
      _pendingScrollData = null;
      sendScroll(data);
    } else {
      _pendingScrollData = data;
      _pendingScrollTimer ??= Timer(Duration(milliseconds: 100 - elapsed), () {
        _pendingScrollTimer = null;
        final pending = _pendingScrollData;
        _pendingScrollData = null;
        if (pending != null) {
          _lastScrollSent = DateTime.now();
          sendScroll(pending);
        }
      });
    }
  }

  void dispose() {
    _pendingScrollTimer?.cancel();
    _pendingScrollTimer = null;
    _pendingScrollData = null;
  }
}

class _ScrollArea extends StatefulWidget {
  const _ScrollArea({required this.build, required this.child});

  final BlockBuild build;
  final Widget child;

  @override
  State<_ScrollArea> createState() => _ScrollAreaState();
}

class _ScrollAreaState extends State<_ScrollArea> {
  late final ScrollController _controller;
  final BlocksScrollThrottle _throttle = BlocksScrollThrottle();
  bool _nearStart = false;
  bool _nearEnd = false;

  @override
  void initState() {
    super.initState();
    _controller = ScrollController();
    _controller.addListener(_checkScroll);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _checkScroll();
    });
  }

  @override
  void dispose() {
    _throttle.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _checkScroll() {
    if (!_controller.hasClients) return;
    final position = _controller.position;
    final pixels = position.pixels;
    final max = position.maxScrollExtent;
    final node = widget.build.node;

    // Check reach_start (within 48 px of 0)
    if (pixels <= 48) {
      if (!_nearStart) {
        _nearStart = true;
        if (node.handles('reach_start')) {
          widget.build.send('reach_start');
        }
      }
    } else {
      _nearStart = false;
    }

    // Check reach_end (within 48 px of max extent)
    if (max > 0 && max - pixels <= 48) {
      if (!_nearEnd) {
        _nearEnd = true;
        if (node.handles('reach_end')) {
          widget.build.send('reach_end');
        }
      }
    } else {
      _nearEnd = false;
    }

    // Handle scroll event (at most 10 a second = 100 ms)
    _throttle.handleScroll(
      position: position,
      handlesScroll: node.handles('scroll'),
      sendScroll: (data) {
        if (mounted) widget.build.send('scroll', data);
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final direction = widget.build.props['direction'] == 'horizontal'
        ? Axis.horizontal
        : Axis.vertical;

    return SingleChildScrollView(
      controller: _controller,
      scrollDirection: direction,
      child: widget.child,
    );
  }
}
