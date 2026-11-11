import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../view.dart';

/// `modal` (design §12.5): in-tree modal block rendered in the overlay above the tree.
final modalBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _InTreeModal(build: b);
}

class _InTreeModal extends StatefulWidget {
  const _InTreeModal({required this.build});

  final BlockBuild build;

  @override
  State<_InTreeModal> createState() => _InTreeModalState();
}

class _InTreeModalState extends State<_InTreeModal>
    with SingleTickerProviderStateMixin {
  late final AnimationController _anim;
  late final Animation<double> _curved;
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
    _curved = CurvedAnimation(parent: _anim, curve: Curves.easeOut);
    _focusScopeNode = FocusScopeNode();
    if (widget.build.props['open'] == true) {
      _portalController.show();
      _anim.forward();
    }
  }

  @override
  void didUpdateWidget(_InTreeModal oldWidget) {
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
        final props = widget.build.props;
        final children = widget.build.node.children;
        if (children.isEmpty) return const SizedBox.shrink();
        final childId = children.first;

        final size = props['size'] as String? ?? 'medium';
        final targetWidth = switch (size) {
          'small' => 360.0,
          'medium' => 520.0,
          'large' => 760.0,
          _ => 520.0,
        };

        return LayoutBuilder(
          builder: (context, constraints) {
            Widget content = Container(
              width: math.min(targetWidth, constraints.maxWidth),
              constraints: BoxConstraints(
                maxHeight: constraints.maxHeight - 32,
              ),
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: theme.colors['elevated']!,
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: theme.colors['border']!, width: 1),
                boxShadow: theme.shadows['lg'],
              ),
              child: ClipRRect(
                borderRadius: BorderRadius.circular(12),
                child: SingleChildScrollView(
                  child: BlockWidget(key: ValueKey<int>(childId), id: childId),
                ),
              ),
            );

            content = ScaleTransition(
              scale: Tween<double>(begin: 0.97, end: 1.0).animate(_curved),
              child: FadeTransition(opacity: _curved, child: content),
            );

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
                    Center(
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
        );
      },
      child: const SizedBox.shrink(),
    );
  }
}
