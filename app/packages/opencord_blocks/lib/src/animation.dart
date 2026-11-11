import 'package:flutter/widgets.dart';

import 'common_props.dart';
import 'view.dart';

/// Supported curve names (§12.4):
Curve resolveCurve(String? name) => switch (name) {
  'linear' => Curves.linear,
  'ease' => Curves.ease,
  'ease_in' => Curves.easeIn,
  'ease_out' => Curves.easeOut,
  'ease_in_out' => Curves.easeInOut,
  'spring' => Curves.elasticOut,
  _ => Curves.easeOut,
};

/// Reads `animate = { duration, curve }` (duration 0–2000 ms, default 200 ms).
/// Returns null if `props` does not contain `animate`.
/// If animations are disabled via [MediaQuery], duration is [Duration.zero].
(Duration, Curve)? readAnimate(
  Map<String, Object?> props,
  BuildContext context,
) {
  final animate = props['animate'];
  if (animate is! Map<String, Object?>) return null;
  final disable = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
  if (disable) {
    return (Duration.zero, Curves.linear);
  }
  final durMs = (animate['duration'] as num?)?.toInt().clamp(0, 2000) ?? 200;
  final curve = resolveCurve(animate['curve'] as String?);
  return (Duration(milliseconds: durMs), curve);
}

/// Applies `enter` or `exit` transition (`fade`, `slide_up`, `slide_down`, `scale`).
Widget buildTransition(
  Widget child,
  Animation<double> animation,
  String? effect,
) => switch (effect) {
  'fade' => FadeTransition(opacity: animation, child: child),
  'slide_up' => SlideTransition(
    position: Tween<Offset>(
      begin: const Offset(0, 0.25),
      end: Offset.zero,
    ).animate(animation),
    child: child,
  ),
  'slide_down' => SlideTransition(
    position: Tween<Offset>(
      begin: const Offset(0, -0.25),
      end: Offset.zero,
    ).animate(animation),
    child: child,
  ),
  'scale' => ScaleTransition(scale: animation, child: child),
  _ => child,
};

/// Wraps [child] with implicit animations for opacity, size, and container
/// decorations when [props] has an `animate` prop.
class AnimatedBlock extends StatefulWidget {
  const AnimatedBlock({
    super.key,
    required this.build,
    required this.props,
    required this.child,
  });

  final BlockBuild build;
  final Map<String, Object?> props;
  final Widget child;

  @override
  State<AnimatedBlock> createState() => _AnimatedBlockState();
}

class _AnimatedBlockState extends State<AnimatedBlock> {
  var _animationEnded = false;

  void _onEnd() {
    if (_animationEnded) return;
    _animationEnded = true;
    final node = widget.build.node;
    if (node.handles('animation_end')) {
      widget.build.send('animation_end');
    }
  }

  @override
  void didUpdateWidget(AnimatedBlock oldWidget) {
    super.didUpdateWidget(oldWidget);
    _animationEnded = false;
  }

  @override
  Widget build(BuildContext context) {
    final anim = readAnimate(widget.props, context);
    if (anim == null) return widget.child;

    final (duration, curve) = anim;
    final props = widget.props;
    final opacity = (numberProp(props['opacity']) ?? 1.0).clamp(0.0, 1.0);
    final width = numberProp(props['width']);
    final height = numberProp(props['height']);

    Widget result = widget.child;

    if (width != null || height != null) {
      result = AnimatedContainer(
        duration: duration,
        curve: curve,
        width: width,
        height: height,
        child: result,
      );
    }

    result = AnimatedOpacity(
      opacity: opacity,
      duration: duration,
      curve: curve,
      onEnd: _onEnd,
      child: result,
    );

    return result;
  }
}

class ChildItem {
  const ChildItem({
    required this.id,
    required this.widget,
    required this.isExiting,
  });

  final int id;
  final Widget widget;
  final bool isExiting;
}

/// Manages keyed children of `column`, `row`, or `list` with `enter` and `exit`
/// animations (`fade`, `slide_up`, `slide_down`, `scale`).
class AnimatedChildren extends StatefulWidget {
  const AnimatedChildren({
    super.key,
    required this.build,
    required this.children,
    required this.childBuilder,
    required this.builder,
  });

  final BlockBuild build;
  final List<int> children;
  final Widget Function(BuildContext context, int id, bool isExiting)
  childBuilder;
  final Widget Function(BuildContext context, List<Widget> children) builder;

  @override
  State<AnimatedChildren> createState() => _AnimatedChildrenState();
}

class _ChildEntry {
  _ChildEntry({
    required this.id,
    required this.key,
    this.controller,
    this.curved,
    this.enterEffect,
    this.exitEffect,
    this.isExiting = false,
    this.lastWidget,
  });

  final int id;
  final String? key;
  final AnimationController? controller;
  final Animation<double>? curved;
  final String? enterEffect;
  final String? exitEffect;
  bool isExiting;
  Widget? lastWidget;
}

class _AnimatedChildrenState extends State<AnimatedChildren>
    with TickerProviderStateMixin {
  final Map<int, _ChildEntry> _entries = {};
  final List<int> _order = [];

  @override
  void initState() {
    super.initState();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disableAnimations =
        MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    _syncChildren(
      widget.children,
      isInitial: _entries.isEmpty,
      disableAnimations: disableAnimations,
    );
  }

  @override
  void didUpdateWidget(AnimatedChildren oldWidget) {
    super.didUpdateWidget(oldWidget);
    final disableAnimations =
        MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    _syncChildren(
      widget.children,
      isInitial: false,
      disableAnimations: disableAnimations,
    );
  }

  @override
  void dispose() {
    for (final entry in _entries.values) {
      entry.controller?.dispose();
    }
    _entries.clear();
    _order.clear();
    super.dispose();
  }

  void _syncChildren(
    List<int> currentIds, {
    required bool isInitial,
    required bool disableAnimations,
  }) {
    final controller = widget.build.controller;

    // Check removed children
    for (final id in List<int>.from(_order)) {
      if (!currentIds.contains(id)) {
        final entry = _entries[id];
        if (entry != null &&
            entry.key != null &&
            entry.exitEffect != null &&
            !entry.isExiting &&
            !disableAnimations) {
          entry.isExiting = true;
          final anim = readAnimate(
            controller.node(id)?.props ?? const {},
            context,
          );
          final duration = anim?.$1 ?? const Duration(milliseconds: 200);
          final curve = anim?.$2 ?? Curves.easeOut;

          var animController = entry.controller;
          if (animController == null) {
            animController = AnimationController(
              vsync: this,
              duration: duration,
              value: 1.0,
            );
          } else {
            animController.duration = duration;
            animController.value = 1.0;
          }
          final curved = CurvedAnimation(parent: animController, curve: curve);

          _entries[id] = _ChildEntry(
            id: id,
            key: entry.key,
            controller: animController,
            curved: curved,
            enterEffect: entry.enterEffect,
            exitEffect: entry.exitEffect,
            isExiting: true,
            lastWidget: entry.lastWidget,
          );

          animController.reverse().then((_) {
            if (!mounted) {
              controller.finishExit(id);
              return;
            }
            final node = controller.node(id);
            if (node != null && node.handles('animation_end')) {
              controller.event(id, 'animation_end');
            }
            controller.finishExit(id);
            _removeEntry(id);
          });
        } else if (entry != null && !entry.isExiting) {
          controller.finishExit(id);
          _removeEntry(id);
        }
      }
    }

    // Check added children
    for (var i = 0; i < currentIds.length; i++) {
      final id = currentIds[i];
      if (!_entries.containsKey(id)) {
        final node = controller.node(id);
        final key = node?.key;
        final enterEffect = node?.props['enter'] as String?;
        final exitEffect = node?.props['exit'] as String?;

        if (key != null &&
            (enterEffect != null || exitEffect != null) &&
            !disableAnimations) {
          final anim = readAnimate(node?.props ?? const {}, context);
          final duration = anim?.$1 ?? const Duration(milliseconds: 200);
          final curve = anim?.$2 ?? Curves.easeOut;

          AnimationController? animController;
          Animation<double>? curved;
          if (!isInitial || enterEffect != null) {
            animController = AnimationController(
              vsync: this,
              duration: duration,
              value: enterEffect != null ? 0.0 : 1.0,
            );
            curved = CurvedAnimation(parent: animController, curve: curve);
            if (enterEffect != null) {
              animController.forward().then((_) {
                if (!mounted) return;
                final currNode = controller.node(id);
                if (currNode != null && currNode.handles('animation_end')) {
                  controller.event(id, 'animation_end');
                }
              });
            }
          }

          final entry = _ChildEntry(
            id: id,
            key: key,
            controller: animController,
            curved: curved,
            enterEffect: enterEffect,
            exitEffect: exitEffect,
          );
          _entries[id] = entry;
        } else {
          final entry = _ChildEntry(
            id: id,
            key: key,
            enterEffect: enterEffect,
            exitEffect: exitEffect,
          );
          _entries[id] = entry;
        }
      } else {
        final entry = _entries[id]!;
        if (entry.isExiting) {
          entry.isExiting = false;
          entry.controller?.forward();
        }
      }
    }

    // Rebuild _order to match currentIds order, retaining any exiting children
    final oldOrder = List<int>.from(_order);
    final exiting = [
      for (final id in oldOrder)
        if (_entries[id]?.isExiting == true && !currentIds.contains(id)) id,
    ];

    _order.clear();
    _order.addAll(currentIds);
    for (final id in exiting) {
      final oldIndex = oldOrder.indexOf(id);
      final prevInOld = oldIndex > 0 ? oldOrder[oldIndex - 1] : null;
      if (prevInOld != null && _order.contains(prevInOld)) {
        final newIndex = _order.indexOf(prevInOld) + 1;
        _order.insert(newIndex, id);
      } else {
        _order.add(id);
      }
    }
  }

  void _removeEntry(int id) {
    final entry = _entries.remove(id);
    entry?.controller?.dispose();
    _order.remove(id);
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final widgets = <Widget>[];
    for (final id in _order) {
      final entry = _entries[id];
      if (entry == null) continue;

      Widget itemWidget = widget.childBuilder(context, id, entry.isExiting);
      entry.lastWidget = itemWidget;

      if (entry.controller != null && entry.curved != null) {
        final effect = entry.isExiting ? entry.exitEffect : entry.enterEffect;
        if (effect != null) {
          itemWidget = AnimatedBuilder(
            animation: entry.curved!,
            builder: (context, child) =>
                buildTransition(child!, entry.curved!, effect),
            child: itemWidget,
          );
        }
      }
      widgets.add(itemWidget);
    }
    return widget.builder(context, widgets);
  }
}
