import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `list` (design §12.2): lazy `ListView` over `count` items with `reverse`,
/// range inputs, blank placeholders of `item_height`, and scroll events.
final listBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _ListWidget(build: b);
}

class _ListWidget extends StatefulWidget {
  const _ListWidget({required this.build});

  final BlockBuild build;

  @override
  State<_ListWidget> createState() => _ListWidgetState();
}

class _ListWidgetState extends State<_ListWidget> {
  late final ScrollController _controller;
  bool _nearStart = false;
  bool _nearEnd = false;
  int _lastSentStart = -1;
  int _lastSentEnd = -1;
  DateTime _lastRangeSent = DateTime.fromMillisecondsSinceEpoch(0);
  Timer? _pendingRangeTimer;
  (int, int)? _pendingRange;

  int _lastCount = 0;

  @override
  void initState() {
    super.initState();
    _controller = ScrollController();
    _controller.addListener(_onScroll);
    final count = _readCount();
    _lastCount = count;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _checkRangeAndScroll();
    });
  }

  int _readCount() {
    final countProp = widget.build.props['count'];
    return countProp is num && countProp.isFinite
        ? countProp.toInt().clamp(0, 1000000000)
        : 0;
  }

  double _readItemHeight() {
    return numberProp(widget.build.props['item_height']) ?? 48.0;
  }

  double _readGap() {
    return numberProp(widget.build.props['gap']) ?? 0.0;
  }

  bool _isReverse() {
    return widget.build.props['reverse'] == true;
  }

  @override
  void didUpdateWidget(_ListWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    final nextCount = _readCount();
    final reverse = _isReverse();
    if (reverse && _controller.hasClients) {
      final pixels = _controller.position.pixels;
      final added = nextCount - _lastCount;
      if (added > 0) {
        if (pixels > 48) {
          // User is scrolled up: keep current items visually in place
          final stride = _readItemHeight() + _readGap();
          final target = pixels + added * stride;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted && _controller.hasClients) {
              _controller.jumpTo(target);
            }
          });
        }
      }
    }
    _lastCount = nextCount;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _checkRangeAndScroll();
    });
  }

  @override
  void dispose() {
    _pendingRangeTimer?.cancel();
    _controller.removeListener(_onScroll);
    _controller.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (!_controller.hasClients) return;
    final position = _controller.position;
    final pixels = position.pixels;
    final max = position.maxScrollExtent;
    final node = widget.build.node;

    // reach_start check
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

    // reach_end check
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

    _checkRangeAndScroll();
  }

  void _checkRangeAndScroll() {
    if (!_controller.hasClients) return;
    final position = _controller.position;
    final pixels = position.pixels;
    final viewport = position.viewportDimension;
    final count = _readCount();
    if (count <= 0) return;

    final stride = _readItemHeight() + _readGap();
    final reverse = _isReverse();

    final int start;
    final int end;

    if (!reverse) {
      final firstVisible = (pixels / stride).floor().clamp(0, count);
      final lastVisible = ((pixels + viewport) / stride).ceil().clamp(0, count);
      start = math.max(0, firstVisible - 20);
      end = math.min(count, lastVisible + 20);
    } else {
      final bottomItem = (count - (pixels / stride).floor()).clamp(0, count);
      final topItem = (count - ((pixels + viewport) / stride).ceil()).clamp(
        0,
        count,
      );
      start = math.max(0, topItem - 20);
      end = math.min(count, bottomItem + 20);
    }

    final safeStart = start.clamp(0, count);
    final safeEnd = end.clamp(safeStart, count);

    _sendRange(safeStart, safeEnd);
  }

  void _sendRange(int start, int end) {
    if (start == _lastSentStart && end == _lastSentEnd) return;
    final now = DateTime.now();
    final elapsed = now.difference(_lastRangeSent).inMilliseconds;
    if (elapsed >= 100) {
      _lastRangeSent = now;
      _pendingRangeTimer?.cancel();
      _pendingRange = null;
      _lastSentStart = start;
      _lastSentEnd = end;
      widget.build.controller.send({
        'kind': 'range',
        'node': widget.build.node.id,
        'start': start,
        'end': end,
      });
    } else {
      _pendingRange = (start, end);
      _pendingRangeTimer ??= Timer(Duration(milliseconds: 100 - elapsed), () {
        _pendingRangeTimer = null;
        if (!mounted) return;
        final pending = _pendingRange;
        _pendingRange = null;
        if (pending != null) {
          _lastRangeSent = DateTime.now();
          _lastSentStart = pending.$1;
          _lastSentEnd = pending.$2;
          widget.build.controller.send({
            'kind': 'range',
            'node': widget.build.node.id,
            'start': pending.$1,
            'end': pending.$2,
          });
        }
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final count = _readCount();
    final itemHeight = _readItemHeight();
    final gap = _readGap();
    final reverse = _isReverse();
    final children = widget.build.node.children;
    final controller = widget.build.controller;

    // Map children to item indices
    int? anchorIndex;
    for (var k = 0; k < children.length; k++) {
      final childId = children[k];
      final childNode = controller.node(childId);
      final key = childNode?.key;
      if (key != null && key.startsWith('#')) {
        final parsed = int.tryParse(key.substring(1));
        if (parsed != null) {
          anchorIndex = parsed - k;
          break;
        }
      }
    }
    final startIndex =
        anchorIndex ?? (reverse ? math.max(0, count - children.length) : 0);
    final itemNodes = <int, int>{};
    for (var k = 0; k < children.length; k++) {
      final childId = children[k];
      final childNode = controller.node(childId);
      final key = childNode?.key;
      int? itemIndex;
      if (key != null && key.startsWith('#')) {
        itemIndex = int.tryParse(key.substring(1));
      }
      itemIndex ??= startIndex + k;
      itemNodes[itemIndex] = childId;
    }

    return ListView.builder(
      controller: _controller,
      reverse: reverse,
      itemCount: count,
      itemBuilder: (context, index) {
        final itemIndex = reverse ? count - 1 - index : index;
        final childId = itemNodes[itemIndex];
        Widget itemWidget;
        if (childId != null) {
          itemWidget = BlockWidget(key: ValueKey<int>(childId), id: childId);
        } else {
          itemWidget = SizedBox(height: itemHeight);
        }

        if (gap > 0 && index < count - 1) {
          itemWidget = Padding(
            padding: EdgeInsets.only(bottom: gap),
            child: itemWidget,
          );
        }
        return itemWidget;
      },
    );
  }
}
