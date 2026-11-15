import 'package:opencord_blocks/opencord_blocks.dart';

/// One open overlay the phone's back can close (§12.5), bottom to top.
sealed class AreaOverlay {
  const AreaOverlay();
}

/// A named modal (`roots.modals`).
class NamedOverlay extends AreaOverlay {
  const NamedOverlay(this.modal);
  final BlocksModalRoot modal;
}

/// An in-tree `ui.modal({open = true})` or `ui.sheet`.
class InTreeOverlay extends AreaOverlay {
  const InTreeOverlay(this.node);
  final BlockNode node;
}

/// A quick dialog waiting for its answer.
class DialogOverlay extends AreaOverlay {
  const DialogOverlay(this.dialog);
  final BlocksDialog dialog;
}

/// The open overlays, bottom to top: the main tree's in-tree ones in tree
/// order, then each named modal followed by the in-tree ones inside it, then
/// the dialogs. Empty before the first roots.
List<AreaOverlay> overlaysOf(BlocksController controller) {
  final roots = controller.roots;
  if (roots == null) return const [];
  final stack = <AreaOverlay>[];
  void walk(int id, {required bool isRoot}) {
    final node = controller.node(id);
    if (node == null) return;
    final overlay = node.block == 'modal' || node.block == 'sheet';
    if (!isRoot && overlay && node.props['open'] == true) {
      stack.add(InTreeOverlay(node));
    }
    for (final child in node.children) {
      walk(child, isRoot: false);
    }
  }

  walk(roots.main, isRoot: false);
  for (final modal in roots.modals) {
    stack.add(NamedOverlay(modal));
    // The named modal's root is its own `modal` block, never opened by `open`.
    walk(modal.id, isRoot: true);
  }
  for (final dialog in controller.dialogs) {
    stack.add(DialogOverlay(dialog));
  }
  return stack;
}
