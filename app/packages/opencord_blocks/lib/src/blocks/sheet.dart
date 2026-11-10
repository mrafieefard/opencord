import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../view.dart';

/// `sheet` (design §12.5): in-tree sheet sliding up from bottom.
final sheetBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  final props = b.props;
  if (props['open'] != true) return const SizedBox.shrink();

  final children = b.node.children;
  if (children.isEmpty) return const SizedBox.shrink();

  return _InTreeSheet(build: b);
}

class _InTreeSheet extends StatefulWidget {
  const _InTreeSheet({required this.build});

  final BlockBuild build;

  @override
  State<_InTreeSheet> createState() => _InTreeSheetState();
}

class _InTreeSheetState extends State<_InTreeSheet>
    with SingleTickerProviderStateMixin {
  late final AnimationController _anim;
  late final Animation<Offset> _slide;
  late final FocusScopeNode _focusScopeNode;

  @override
  void initState() {
    super.initState();
    _anim = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 180),
    )..forward();
    _slide = Tween<Offset>(
      begin: const Offset(0, 1),
      end: Offset.zero,
    ).animate(CurvedAnimation(parent: _anim, curve: Curves.easeOut));
    _focusScopeNode = FocusScopeNode();
  }

  @override
  void dispose() {
    _focusScopeNode.dispose();
    _anim.dispose();
    super.dispose();
  }

  void _dismiss() {
    final dismissible = widget.build.props['dismissible'] != false;
    if (dismissible && widget.build.node.handles('dismiss')) {
      widget.build.send('dismiss');
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = widget.build.theme;
    final children = widget.build.node.children;
    final childId = children.first;

    Widget content = Container(
      width: double.infinity,
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: theme.colors['elevated']!,
        borderRadius: const BorderRadius.vertical(top: Radius.circular(16)),
        border: Border.all(color: theme.colors['border']!, width: 1),
        boxShadow: theme.shadows['lg'],
      ),
      child: BlockWidget(key: ValueKey<int>(childId), id: childId),
    );

    content = SlideTransition(position: _slide, child: content);

    return CallbackShortcuts(
      bindings: {const SingleActivator(LogicalKeyboardKey.escape): _dismiss},
      child: FocusScope(
        node: _focusScopeNode,
        autofocus: true,
        child: Stack(
          fit: StackFit.expand,
          children: [
            GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: _dismiss,
              child: Container(color: Colors.black54),
            ),
            Align(
              alignment: Alignment.bottomCenter,
              child: GestureDetector(
                onTap: () {},
                behavior: HitTestBehavior.opaque,
                child: content,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
