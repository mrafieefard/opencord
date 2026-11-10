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

class _ScrollArea extends StatefulWidget {
  const _ScrollArea({required this.build, required this.child});

  final BlockBuild build;
  final Widget child;

  @override
  State<_ScrollArea> createState() => _ScrollAreaState();
}

class _ScrollAreaState extends State<_ScrollArea> {
  late final ScrollController _controller;
  bool _nearStart = false;
  bool _nearEnd = false;
  DateTime _lastScrollSent = DateTime.fromMillisecondsSinceEpoch(0);
  Timer? _pendingScrollTimer;
  Map<String, Object?>? _pendingScrollData;

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
    _pendingScrollTimer?.cancel();
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
    if (node.handles('scroll')) {
      final data = {'offset': pixels, 'extent': max};
      final now = DateTime.now();
      final elapsed = now.difference(_lastScrollSent).inMilliseconds;
      if (elapsed >= 100) {
        _lastScrollSent = now;
        _pendingScrollTimer?.cancel();
        _pendingScrollData = null;
        widget.build.send('scroll', data);
      } else {
        _pendingScrollData = data;
        _pendingScrollTimer ??= Timer(
          Duration(milliseconds: 100 - elapsed),
          () {
            _pendingScrollTimer = null;
            if (!mounted) return;
            final pending = _pendingScrollData;
            _pendingScrollData = null;
            if (pending != null) {
              _lastScrollSent = DateTime.now();
              widget.build.send('scroll', pending);
            }
          },
        );
      }
    }
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
