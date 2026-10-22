import 'package:flutter/widgets.dart';

import 'package:opencord/features/chat/chat_row_view.dart';
import 'package:opencord/features/chat/message_rows.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

/// Registers a built row under its key while it exists, so the list can
/// find what is on screen: the date at the top, a message to jump to.
class TrackedRow extends StatefulWidget {
  const TrackedRow({
    super.key,
    required this.row,
    required this.registry,
    required this.child,
  });

  final ChatRow row;
  final Map<Object, TrackedRowState> registry;
  final Widget child;

  @override
  State<TrackedRow> createState() => TrackedRowState();
}

class TrackedRowState extends State<TrackedRow> {
  ChatRow get row => widget.row;

  @override
  void initState() {
    super.initState();
    widget.registry[widget.row.key] = this;
  }

  @override
  void didUpdateWidget(TrackedRow oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.row.key != widget.row.key ||
        !identical(oldWidget.registry, widget.registry)) {
      _forget(oldWidget);
    }
    widget.registry[widget.row.key] = this;
  }

  @override
  void dispose() {
    _forget(widget);
    super.dispose();
  }

  void _forget(TrackedRow from) {
    if (identical(from.registry[from.row.key], this)) {
      from.registry.remove(from.row.key);
    }
  }

  /// Where the row is, relative to [ancestor]; null when it is not laid
  /// out.
  Rect? rectIn(RenderBox ancestor) {
    final box = context.findRenderObject();
    if (box is! RenderBox || !box.attached || !box.hasSize) return null;
    return box.localToGlobal(Offset.zero, ancestor: ancestor) & box.size;
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// Fades a message in when it arrives and out when it is deleted (100 ms,
/// §2.4, §6).
class RowFade extends StatefulWidget {
  const RowFade({
    super.key,
    required this.fadeIn,
    required this.gone,
    required this.child,
  });

  final bool fadeIn;
  final bool gone;
  final Widget child;

  @override
  State<RowFade> createState() => _RowFadeState();
}

class _RowFadeState extends State<RowFade> {
  late bool _shown = !widget.fadeIn;

  @override
  void initState() {
    super.initState();
    if (!_shown) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) setState(() => _shown = true);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final duration = OcMotion.of(context).message;
    if (duration == Duration.zero) return widget.child;
    return AnimatedOpacity(
      opacity: _shown && !widget.gone ? 1 : 0,
      duration: duration,
      child: widget.child,
    );
  }
}

/// Stands in for the history above while it loads (§4.13).
class OlderHistorySkeleton extends StatelessWidget {
  const OlderHistorySkeleton({super.key});

  @override
  Widget build(BuildContext context) => const Padding(
    padding: EdgeInsets.only(bottom: OcSpace.s8),
    child: ChatSkeleton(rows: 3),
  );
}
