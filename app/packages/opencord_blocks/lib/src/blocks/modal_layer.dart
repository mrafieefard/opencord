import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../controller.dart';
import '../theme.dart';
import '../view.dart';
import 'dialogs.dart';

/// Modal layer (§12.5): draws named modals from [BlocksRoots.modals] over a
/// dimmed backdrop inside the plugin area. Focus stays trapped in the top modal,
/// opening scales from 0.97 and fades in 180 ms ease-out, and closing is instant.
class ModalLayer extends StatelessWidget {
  const ModalLayer({
    super.key,
    required this.controller,
    required this.theme,
    required this.constraints,
  });

  final BlocksController controller;
  final BlocksTheme theme;
  final BoxConstraints constraints;

  @override
  Widget build(BuildContext context) {
    final modals = controller.roots?.modals ?? const [];
    final dialogs = controller.dialogs;

    if (modals.isEmpty && dialogs.isEmpty) {
      return const SizedBox.shrink();
    }

    final isCompact = constraints.maxWidth < 600;

    return Stack(
      fit: StackFit.expand,
      children: [
        for (var i = 0; i < modals.length; i++)
          _ModalWidget(
            key: ValueKey<String>('modal_${modals[i].name}'),
            modal: modals[i],
            isTop: i == modals.length - 1 && dialogs.isEmpty,
            isCompact: isCompact,
            constraints: constraints,
            controller: controller,
            theme: theme,
          ),
        QuickDialogsLayer(controller: controller, theme: theme),
      ],
    );
  }
}

class _ModalWidget extends StatefulWidget {
  const _ModalWidget({
    super.key,
    required this.modal,
    required this.isTop,
    required this.isCompact,
    required this.constraints,
    required this.controller,
    required this.theme,
  });

  final BlocksModalRoot modal;
  final bool isTop;
  final bool isCompact;
  final BoxConstraints constraints;
  final BlocksController controller;
  final BlocksTheme theme;

  @override
  State<_ModalWidget> createState() => _ModalWidgetState();
}

class _ModalWidgetState extends State<_ModalWidget>
    with SingleTickerProviderStateMixin {
  late final AnimationController _anim;
  late final Animation<double> _curved;
  late final FocusScopeNode _focusScopeNode;

  @override
  void initState() {
    super.initState();
    _anim = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 180),
    )..forward();
    _curved = CurvedAnimation(parent: _anim, curve: Curves.easeOut);
    _focusScopeNode = FocusScopeNode();
  }

  @override
  void dispose() {
    _focusScopeNode.dispose();
    _anim.dispose();
    super.dispose();
  }

  void _dismiss() {
    if (widget.modal.dismissible) {
      widget.controller.send({
        'kind': 'modal_dismiss',
        'name': widget.modal.name,
      });
    }
  }

  /// What the card holds. A named modal's root is the `ui.modal` block its
  /// function returned: its title and its child are drawn here, since that
  /// block's own renderer is the in-tree modal, which shows only when
  /// `open`. Any other root is drawn as it is.
  Widget _rootContent(BlocksModalRoot modal) {
    final node = widget.controller.node(modal.id);
    if (node == null || node.block != 'modal') {
      return BlockWidget(key: ValueKey<int>(modal.id), id: modal.id);
    }
    final title = node.props['title'];
    final theme = widget.theme;
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (title is String && title.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Text(
                title,
                style: theme.text.copyWith(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                  color: theme.colors['text'],
                ),
              ),
            ),
          for (final child in node.children)
            BlockWidget(key: ValueKey<int>(child), id: child),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final modal = widget.modal;
    final theme = widget.theme;
    final constraints = widget.constraints;

    // The renderer shows a sheet when sheet is true, or at compact width (< 600 px) when it isn't false
    final isSheet =
        modal.sheet == true || (widget.isCompact && modal.sheet != false);

    final double width;
    if (modal.size == 'full') {
      width = constraints.maxWidth;
    } else if (isSheet) {
      width = constraints.maxWidth;
    } else {
      final target = switch (modal.size) {
        'small' => 360.0,
        'medium' => 520.0,
        'large' => 760.0,
        _ => 520.0,
      };
      width = math.min(target, constraints.maxWidth);
    }

    Widget content = Container(
      key: ValueKey<String>('modal_card_${modal.name}'),
      width: width,
      constraints: BoxConstraints(
        maxHeight: constraints.maxHeight,
        minHeight: modal.size == 'full' ? constraints.maxHeight : 0,
      ),
      decoration: BoxDecoration(
        color: theme.colors['elevated']!,
        borderRadius: isSheet
            ? const BorderRadius.vertical(top: Radius.circular(16))
            : (modal.size == 'full'
                  ? BorderRadius.zero
                  : BorderRadius.circular(12)),
        border: modal.size == 'full'
            ? null
            : Border.all(color: theme.colors['border']!, width: 1),
        boxShadow: theme.shadows['lg'],
      ),
      child: ClipRRect(
        borderRadius: isSheet
            ? const BorderRadius.vertical(top: Radius.circular(16))
            : (modal.size == 'full'
                  ? BorderRadius.zero
                  : BorderRadius.circular(12)),
        child: SingleChildScrollView(child: _rootContent(modal)),
      ),
    );

    // Opening animation: sheet slides up from bottom; modal card scales from 0.97 and fades
    if (isSheet) {
      content = SlideTransition(
        position: Tween<Offset>(
          begin: const Offset(0, 1),
          end: Offset.zero,
        ).animate(_curved),
        child: content,
      );
    } else {
      content = ScaleTransition(
        scale: Tween<double>(begin: 0.97, end: 1.0).animate(_curved),
        child: FadeTransition(opacity: _curved, child: content),
      );
    }

    final alignment = isSheet
        ? Alignment.bottomCenter
        : (modal.size == 'full' ? Alignment.topLeft : Alignment.center);

    Widget modalTree = Stack(
      fit: StackFit.expand,
      children: [
        // Backdrop
        GestureDetector(
          key: ValueKey<String>('modal_backdrop_${modal.name}'),
          behavior: HitTestBehavior.opaque,
          onTap: _dismiss,
          child: Container(color: Colors.black54),
        ),
        // Content container
        Align(
          alignment: alignment,
          child: GestureDetector(
            onTap: () {}, // Don't propagate tap to backdrop
            behavior: HitTestBehavior.opaque,
            child: content,
          ),
        ),
      ],
    );

    if (widget.isTop) {
      modalTree = CallbackShortcuts(
        bindings: {const SingleActivator(LogicalKeyboardKey.escape): _dismiss},
        child: FocusScope(
          node: _focusScopeNode,
          autofocus: true,
          child: modalTree,
        ),
      );
    }

    return modalTree;
  }
}
