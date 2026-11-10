import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart' show Tooltip;
import 'package:flutter/widgets.dart';

import 'blocks/badge_progress_spinner.dart';
import 'blocks/box.dart';
import 'blocks/canvas.dart';
import 'blocks/grid.dart';
import 'blocks/icon.dart';
import 'blocks/image.dart';
import 'blocks/rich_text.dart';
import 'blocks/row_column.dart';
import 'blocks/scroll.dart';
import 'blocks/spacer_divider.dart';
import 'blocks/stack.dart';
import 'blocks/text.dart';
import 'blocks/unknown.dart';
import 'blocks/user.dart';
import 'common_props.dart';
import 'controller.dart';
import 'frames.dart';
import 'host.dart';
import 'node.dart';
import 'theme.dart';

/// Draws one view: its main tree and, above it, its modals (Task 22), all
/// inside its own box. Overlays its blocks open (tooltips) use the view's
/// own [Overlay], so nothing a plugin draws leaves the plugin area. Tells
/// the core the area's size, theme, input kind, safe area and text scale
/// whenever they change, hands the view's side effects to [host], and sends
/// `frame` inputs while the view's frames are on ([FrameLoop]).
class BlocksView extends StatelessWidget {
  const BlocksView({
    super.key,
    required this.controller,
    required this.host,
    required this.theme,
  });

  final BlocksController controller;
  final BlocksHost host;
  final BlocksTheme theme;

  @override
  Widget build(BuildContext context) =>
      _BlocksArea(controller: controller, host: host, theme: theme);
}

class _BlocksArea extends StatefulWidget {
  const _BlocksArea({
    required this.controller,
    required this.host,
    required this.theme,
  });

  final BlocksController controller;
  final BlocksHost host;
  final BlocksTheme theme;

  @override
  State<_BlocksArea> createState() => _BlocksAreaState();
}

class _BlocksAreaState extends State<_BlocksArea>
    with TickerProviderStateMixin {
  String? _lastContext;
  String? _waitingContext;
  late FrameLoop _frames;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_takeEffects);
    _frames = FrameLoop(widget.controller, this);
    // Effects from batches applied before this view was shown.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _takeEffects();
    });
  }

  @override
  void didUpdateWidget(_BlocksArea oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_takeEffects);
      widget.controller.addListener(_takeEffects);
      _frames.dispose();
      _frames = FrameLoop(widget.controller, this);
      _lastContext = null;
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_takeEffects);
    _frames.dispose();
    super.dispose();
  }

  void _takeEffects() {
    final controller = widget.controller;
    for (final effect in controller.takeEffects()) {
      switch (effect) {
        case CopyEffect(:final text):
          widget.host.copy(text);
        case OpenLinkEffect(:final url):
          _watch(widget.host.confirmLink(url), 'opening a link');
        case OpenChannelEffect(:final channelId):
          widget.host.openChannel(channelId);
        case SoundEffect(:final id, :final asset, :final volume):
          var ended = false;
          void end() {
            if (ended) return;
            ended = true;
            controller.send({'kind': 'sound_end', 'id': id});
          }
          _watch(
            widget.host.playSound(asset, volume, end),
            'playing a sound',
            // A sound that can't play has ended, so the plugin's
            // `on_end` runs and its wait leaves.
            onError: end,
          );
      }
    }
  }

  /// Reports a host call's failure to Flutter's error handler (the app's
  /// log); nothing else waits for it.
  void _watch(Future<Object?> work, String what, {VoidCallback? onError}) {
    work.then<void>(
      (_) {},
      onError: (Object error, StackTrace stack) {
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stack,
            library: 'opencord_blocks',
            context: ErrorDescription('while $what for a plugin view'),
          ),
        );
        onError?.call();
      },
    );
  }

  void _reportContext(BuildContext context, BoxConstraints constraints) {
    final window = MediaQuery.sizeOf(context);
    final safe = MediaQuery.paddingOf(context);
    final input = switch (defaultTargetPlatform) {
      TargetPlatform.android ||
      TargetPlatform.iOS ||
      TargetPlatform.fuchsia => 'touch',
      _ => 'mouse',
    };
    final json = jsonEncode({
      'kind': 'context',
      'width': constraints.hasBoundedWidth
          ? constraints.maxWidth
          : window.width,
      'height': constraints.hasBoundedHeight
          ? constraints.maxHeight
          : window.height,
      'theme': widget.theme.brightness == Brightness.dark ? 'dark' : 'light',
      'input': input,
      'safe_area': {
        'top': safe.top,
        'right': safe.right,
        'bottom': safe.bottom,
        'left': safe.left,
      },
      'text_scale': MediaQuery.textScalerOf(context).scale(1),
    });
    if (json == _lastContext || json == _waitingContext) return;
    final first = _waitingContext == null;
    _waitingContext = json;
    if (!first) return;
    // Sent after the frame, once, with the newest values.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final waiting = _waitingContext;
      _waitingContext = null;
      if (!mounted || waiting == null || waiting == _lastContext) return;
      _lastContext = waiting;
      widget.controller.send((jsonDecode(waiting) as Map<String, Object?>));
    });
  }

  @override
  Widget build(BuildContext context) {
    return BlocksScope(
      controller: widget.controller,
      host: widget.host,
      theme: widget.theme,
      child: LayoutBuilder(
        builder: (context, constraints) {
          _reportContext(context, constraints);
          // The theme's text replaces whatever the app around set, so no
          // style leaks in (or Flutter's "no style" underline).
          return DefaultTextStyle(
            style: widget.theme.text,
            child: ClipRect(
              child: Overlay.wrap(
                child: ListenableBuilder(
                  listenable: widget.controller,
                  builder: (context, _) {
                    final main = widget.controller.roots?.main;
                    if (main == null) return const SizedBox.shrink();
                    return Align(
                      alignment: AlignmentDirectional.topStart,
                      child: BlockWidget(key: ValueKey<int>(main), id: main),
                    );
                  },
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}

/// What every block in a view can reach.
class BlocksScope extends InheritedWidget {
  const BlocksScope({
    super.key,
    required this.controller,
    required this.host,
    required this.theme,
    required super.child,
  });

  final BlocksController controller;
  final BlocksHost host;
  final BlocksTheme theme;

  static BlocksScope of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<BlocksScope>()!;

  @override
  bool updateShouldNotify(BlocksScope oldWidget) =>
      controller != oldWidget.controller ||
      host != oldWidget.host ||
      theme != oldWidget.theme;
}

/// One block's drawing, for one build of its node.
class BlockBuild {
  BlockBuild(this.context, this.scope, this.node);

  final BuildContext context;
  final BlocksScope scope;
  final BlockNode node;

  BlocksController get controller => scope.controller;
  BlocksHost get host => scope.host;
  BlocksTheme get theme => scope.theme;
  Map<String, Object?> get props => node.props;

  /// Sends one of this block's events, if the plugin handles it now.
  void send(String event, [Map<String, Object?> data = const {}]) =>
      controller.event(node.id, event, data);
}

/// How one kind of block draws: [content] inside its padding and container
/// props, and [around] (presses, hovers) outside them, inside its size.
class BlockRenderer {
  const BlockRenderer(this.content, {this.around});

  final Widget Function(BlockBuild b) content;
  final Widget Function(BlockBuild b, Widget drawn)? around;
}

/// The blocks this package draws; anything else is drawn by [unknownBlock].
final Map<String, BlockRenderer> blockRenderers = {
  'row': rowBlock,
  'column': columnBlock,
  'box': boxBlock,
  'canvas': canvasBlock,
  'stack': stackBlock,
  'grid': gridBlock,
  'scroll': scrollBlock,
  'spacer': spacerBlock,
  'divider': dividerBlock,
  'text': textBlock,
  'rich_text': richTextBlock,
  'image': imageBlock,
  'icon': iconBlock,
  'user': userBlock,
  'badge': badgeBlock,
  'progress': progressBlock,
  'spinner': spinnerBlock,
};

/// Draws the node [id] and redraws it when it changes. Its common props
/// apply here: size, padding, container props, opacity, `visible`,
/// `tooltip` and `label`. [stretch] fills the axis given when the node sets
/// no size on it (its parent's `cross = "stretch"`).
class BlockWidget extends StatefulWidget {
  const BlockWidget({super.key, required this.id, this.stretch});

  final int id;
  final Axis? stretch;

  @override
  State<BlockWidget> createState() => _BlockWidgetState();
}

class _BlockWidgetState extends State<BlockWidget> {
  BlocksScope? _scope;
  ValueListenable<BlockNode?>? _watched;
  BlockNode? _builtFor;
  Widget? _built;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final scope = BlocksScope.of(context);
    if (scope.controller != _scope?.controller) _subscribe(scope.controller);
    _scope = scope;
    _built = null;
  }

  @override
  void didUpdateWidget(BlockWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.id != widget.id) _subscribe(_scope!.controller);
    if (oldWidget.id != widget.id || oldWidget.stretch != widget.stretch) {
      _built = null;
    }
  }

  @override
  void dispose() {
    _watched?.removeListener(_changed);
    super.dispose();
  }

  void _subscribe(BlocksController controller) {
    _watched?.removeListener(_changed);
    _watched = controller.watch(widget.id)?..addListener(_changed);
    _built = null;
  }

  void _changed() => setState(() => _built = null);

  @override
  Widget build(BuildContext context) {
    final node = _watched?.value;
    if (node == null) return const SizedBox.shrink();
    final built = _built;
    // The same node draws the same widget, so Flutter stops here and its
    // children, which watch their own nodes, aren't built again.
    if (built != null && identical(node, _builtFor)) return built;
    _builtFor = node;
    return _built = _draw(BlockBuild(context, _scope!, node));
  }

  Widget _draw(BlockBuild b) {
    final props = b.props;
    if (props['visible'] == false) return const SizedBox.shrink();
    final renderer = blockRenderers[b.node.block] ?? unknownBlock;
    var drawn = renderer.content(b);
    if (insetsProp(props['padding']) case final padding?) {
      drawn = Padding(padding: padding, child: drawn);
    }
    if (containerBlocks.contains(b.node.block)) {
      drawn = decorate(b.theme, props, drawn);
    }
    if (renderer.around case final around?) drawn = around(b, drawn);
    final width = AxisSize.read(
      props,
      'width',
      stretched: widget.stretch == Axis.horizontal,
    );
    final height = AxisSize.read(
      props,
      'height',
      stretched: widget.stretch == Axis.vertical,
    );
    if (!width.isFree || !height.isFree) {
      drawn = BlockSize(width: width, height: height, child: drawn);
    }
    if (numberProp(props['opacity']) case final opacity? when opacity < 1) {
      drawn = Opacity(opacity: opacity, child: drawn);
    }
    if (props['tooltip'] case final String tooltip when tooltip.isNotEmpty) {
      drawn = Tooltip(message: tooltip, child: drawn);
    }
    if (props['label'] case final String label when label.isNotEmpty) {
      drawn = Semantics(container: true, label: label, child: drawn);
    }
    return drawn;
  }
}
