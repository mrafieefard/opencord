import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../view.dart';

/// `sheet` (design §12.5): in-tree sheet sliding up from bottom in the overlay.
final sheetBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
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
  late final OverlayPortalController _portalController;

  @override
  void initState() {
    super.initState();
    _portalController = OverlayPortalController();
    _anim = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 180),
    );
    _slide = Tween<Offset>(
      begin: const Offset(0, 1),
      end: Offset.zero,
    ).animate(CurvedAnimation(parent: _anim, curve: Curves.easeOut));
    _focusScopeNode = FocusScopeNode();
    if (widget.build.props['open'] == true) {
      _portalController.show();
      _anim.forward();
    }
  }

  @override
  void didUpdateWidget(_InTreeSheet oldWidget) {
    super.didUpdateWidget(oldWidget);
    final isOpen = widget.build.props['open'] == true;
    if (isOpen && !_portalController.isShowing) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) {
          _portalController.show();
          _anim.forward(from: 0.0);
        }
      });
    } else if (!isOpen && _portalController.isShowing) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) {
          _portalController.hide();
          _anim.reset();
        }
      });
    }
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
    return OverlayPortal(
      controller: _portalController,
      overlayChildBuilder: (context) {
        final theme = widget.build.theme;
        final children = widget.build.node.children;
        if (children.isEmpty) return const SizedBox.shrink();
        final childId = children.first;

        Widget content = LayoutBuilder(
          builder: (context, constraints) {
            return Container(
              width: double.infinity,
              constraints: BoxConstraints(
                maxHeight: constraints.maxHeight * 0.9,
              ),
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: theme.colors['elevated']!,
                borderRadius: const BorderRadius.vertical(
                  top: Radius.circular(16),
                ),
                border: Border.all(color: theme.colors['border']!, width: 1),
                boxShadow: theme.shadows['lg'],
              ),
              child: SingleChildScrollView(
                child: BlockWidget(key: ValueKey<int>(childId), id: childId),
              ),
            );
          },
        );

        content = SlideTransition(position: _slide, child: content);

        return CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.escape): _dismiss,
          },
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
      },
      child: const SizedBox.shrink(),
    );
  }
}
