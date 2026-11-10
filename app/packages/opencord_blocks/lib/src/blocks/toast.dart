import 'dart:async';

import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `toast` (design §12.2): floating notification at the bottom of the area
/// that dismisses after `duration` seconds.
final toastBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _ToastWidget(build: b);
}

class _ToastWidget extends StatefulWidget {
  const _ToastWidget({required this.build});

  final BlockBuild build;

  @override
  State<_ToastWidget> createState() => _ToastWidgetState();
}

class _ToastWidgetState extends State<_ToastWidget> {
  Timer? _dismissTimer;

  @override
  void initState() {
    super.initState();
    _startTimer();
  }

  @override
  void didUpdateWidget(_ToastWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    _dismissTimer?.cancel();
    _startTimer();
  }

  @override
  void dispose() {
    _dismissTimer?.cancel();
    super.dispose();
  }

  void _startTimer() {
    final seconds = numberProp(widget.build.props['duration']) ?? 3.0;
    _dismissTimer = Timer(Duration(milliseconds: (seconds * 1000).toInt()), () {
      if (!mounted) return;
      final node = widget.build.node;
      if (node.handles('dismiss')) {
        widget.build.send('dismiss');
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = widget.build.theme;
    final props = widget.build.props;
    final text = props['text'] as String? ?? '';
    final children = widget.build.node.children;

    Widget content;
    if (children.isNotEmpty) {
      content = BlockWidget(id: children.first);
    } else {
      content = Text(
        text,
        style: TextStyle(
          fontFamily: theme.text.fontFamily ?? 'Inter',
          fontSize: 14,
          fontWeight: FontWeight.w500,
          color: theme.colors['on_accent']!,
        ),
      );
    }

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      decoration: BoxDecoration(
        color: theme.colors['accent']!,
        borderRadius: BorderRadius.circular(8),
        boxShadow: theme.shadows['md'],
      ),
      child: content,
    );
  }
}
