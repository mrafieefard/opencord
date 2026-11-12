import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/plugins/blocks_host.dart';
import 'package:opencord/features/plugins/blocks_theme.dart';
import 'package:opencord/features/shell/shortcuts.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

/// The plugin area for a plugin view: a custom channel's, or an activity's
/// in a call. Renders BlocksView or state placeholders.
class PluginArea extends ConsumerStatefulWidget {
  const PluginArea({super.key, required this.place, this.paused = false});

  final ViewPlaceRef place;

  /// Whether the view isn't drawn where its user can see it (an activity in
  /// focus mode's strip). A change is told to the view as `pause` or
  /// `resume`; the window being hidden pauses it too, and it's resumed only
  /// when neither holds.
  final bool paused;

  @override
  ConsumerState<PluginArea> createState() => PluginAreaState();
}

class PluginAreaState extends ConsumerState<PluginArea>
    with WidgetsBindingObserver {
  late final FocusNode _focusNode;
  OpencordBlocksHost? _blocksHost;

  /// The window is hidden or minimized.
  bool _windowHidden = false;

  /// What the view was last told: `pause` (true) or `resume` (false). A
  /// view starts running.
  bool _toldPaused = false;

  ViewPlaceRef get _place => widget.place;

  /// The plugin channel this area draws, or null for an activity.
  Channel? _channel(ServerData? serverData) => switch (_place) {
    ChannelPlace(:final channel) => serverData?.channels[channel],
    ActivityPlace() => null,
  };

  /// Whether the view can be told anything: a channel's only while its
  /// plugin provides it (§14.4); an activity's always.
  bool get _viewRuns => switch (_place) {
    ChannelPlace() =>
      _channel(ref.read(serverProvider(_place.server)).data)?.pluginState ==
          PluginChannelState.available,
    ActivityPlace() => true,
  };

  BlocksHost get blocksHost {
    final serverData = ref.read(serverProvider(_place.server)).data;
    final pluginId = switch (_place) {
      ChannelPlace() => _channel(serverData)?.pluginId ?? '',
      ActivityPlace(:final session) => session.pluginId,
    };
    final controller = ref.read(pluginViewProvider(_place));
    return _blocksHost ??= OpencordBlocksHost(
      context: context,
      ref: ref,
      serverKey: _place.server,
      pluginId: pluginId,
      viewId: controller.viewId,
    );
  }

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'plugin_area');
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didUpdateWidget(PluginArea oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.paused != widget.paused) _tellPaused();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _focusNode.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    switch (state) {
      case AppLifecycleState.resumed:
        _windowHidden = false;
      case AppLifecycleState.paused:
      case AppLifecycleState.hidden:
        _windowHidden = true;
      case AppLifecycleState.inactive:
      case AppLifecycleState.detached:
        return;
    }
    _tellPaused();
  }

  /// Tells the view `pause` or `resume` when whether it should run changed
  /// since it was last told: once per change, whichever of the window and
  /// [PluginArea.paused] caused it.
  void _tellPaused() {
    final pause = _windowHidden || widget.paused;
    if (pause == _toldPaused || !_viewRuns) return;
    _toldPaused = pause;
    ref
        .read(pluginViewProvider(_place))
        .send({'kind': 'lifecycle', 'event': pause ? 'pause' : 'resume'});
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent &&
        event is! KeyUpEvent &&
        event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }

    if (!_focusNode.hasPrimaryFocus) {
      return KeyEventResult.ignored;
    }

    // Check if any shell shortcut matches this event
    final shortcuts = shellShortcuts(Theme.of(context).platform);
    for (final activator in shortcuts.keys) {
      if (activator.accepts(event, HardwareKeyboard.instance)) {
        return KeyEventResult.ignored;
      }
    }

    final controller = ref.read(pluginViewProvider(_place));

    final keyboard = HardwareKeyboard.instance;
    final char = event.character;
    final keyLabel = (char != null && char.isNotEmpty && char.trim().isNotEmpty)
        ? char
        : event.logicalKey.keyLabel;
    final code = event.physicalKey.debugName?.replaceAll(' ', '') ?? '';

    controller.send({
      'kind': 'key',
      'event': event is KeyUpEvent ? 'key_up' : 'key_down',
      'key': keyLabel,
      'code': code,
      'shift': keyboard.isShiftPressed,
      'ctrl': keyboard.isControlPressed,
      'alt': keyboard.isAltPressed,
      'meta': keyboard.isMetaPressed,
      'repeat': event is KeyRepeatEvent,
    });
    return KeyEventResult.handled;
  }

  Widget _stateMessage(
    BuildContext context, {
    required String message,
    IconData icon = OcIcons.info,
    Widget? action,
    String? details,
  }) {
    final colors = context.oc;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 48, color: colors.textMuted),
              const SizedBox(height: 16),
              Text(
                message,
                textAlign: TextAlign.center,
                style: OcText.header.copyWith(fontSize: 16, color: colors.text),
              ),
              if (details != null && details.isNotEmpty) ...[
                const SizedBox(height: 12),
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: colors.surface,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: colors.border),
                  ),
                  child: SelectableText(
                    details,
                    style: OcText.mono.copyWith(
                      fontSize: 12,
                      color: colors.textSecondary,
                    ),
                  ),
                ),
              ],
              if (action != null) ...[const SizedBox(height: 20), action],
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final serverData = ref.watch(serverProvider(_place.server)).data;
    final channel = _channel(serverData);

    // Window focus lifecycle monitoring
    ref.listen(windowStatusProvider.select((s) => s.focused), (
      before,
      focused,
    ) {
      if (!_viewRuns) return;
      final controller = ref.read(pluginViewProvider(_place));
      if (focused) {
        controller.send({'kind': 'lifecycle', 'event': 'focus'});
      } else {
        controller.send({'kind': 'lifecycle', 'event': 'blur'});
      }
    });

    // 1. Channel model unavailable states (§14.4); an activity has none
    // (its channel is null here).
    if (channel?.pluginState == PluginChannelState.typeGone) {
      return _stateMessage(
        context,
        message: 'This plugin no longer provides this channel',
        icon: OcIcons.extension,
      );
    }
    if (channel?.pluginState == PluginChannelState.pluginStopped) {
      return _stateMessage(
        context,
        message: 'This channel is unavailable',
        icon: OcIcons.extension,
      );
    }

    final place = _place;
    final controller = ref.watch(pluginViewProvider(place));

    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        // 2. Closed by server (§14.3)
        if (controller.closed case final reason?) {
          // Its viewer left the call: the activity's tile is going.
          if (reason == 'left_call') return const SizedBox.shrink();
          final message = switch (reason) {
            'plugin_stopped' => 'This plugin stopped',
            'lost_access' => 'You can no longer see this channel',
            'channel_deleted' => 'This channel was deleted',
            'activity_ended' => 'This activity ended',
            _ => 'This channel is unavailable',
          };
          return _stateMessage(
            context,
            message: message,
            icon: OcIcons.extension,
          );
        }

        // 3. Too old state
        if (controller.tooOld) {
          return _stateMessage(
            context,
            message: 'Update Opencord to open this',
            icon: OcIcons.warning,
          );
        }

        // 4. Error state
        if (controller.error case final err?) {
          final canSeeDetails =
              serverData?.isOwner == true ||
              serverData?.can(Permissions.administrator) == true ||
              serverData?.can(Permissions.managePlugins) == true;
          return _stateMessage(
            context,
            message: 'This plugin hit an error',
            icon: OcIcons.error,
            details: canSeeDetails ? err.details : null,
            action: OcButton.primary(
              label: 'Reload',
              onPressed: () {
                ref.invalidate(pluginViewProvider(place));
              },
            ),
          );
        }

        // 5. Loading and Running state
        final host = blocksHost;
        final theme = buildBlocksTheme(
          colors,
          brightness: Theme.of(context).brightness,
        );

        return GestureDetector(
          onTapDown: (_) => _focusNode.requestFocus(),
          behavior: HitTestBehavior.translucent,
          child: Focus(
            focusNode: _focusNode,
            onKeyEvent: _handleKeyEvent,
            child: ColoredBox(
              color: colors.chat,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  BlocksView(controller: controller, host: host, theme: theme),
                  if (controller.roots == null)
                    const Center(child: OcSpinner()),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}
