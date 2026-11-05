import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/voice/camera_menu.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// Scope and activity notifier for the voice dock (§5.14).
///
/// Moving the pointer anywhere over the main float shows the dock and
/// restarts a 2.5 s timer. Hides on exit or timer expire, unless held by
/// a pointer on it, keyboard focus inside it, or an open popup.
class VoiceDockActivityNotifier extends ChangeNotifier {
  VoiceDockActivityNotifier({
    this.hideDelay = const Duration(milliseconds: 2500),
  }) {
    _resetHideTimer();
  }

  final Duration hideDelay;
  Timer? _hideTimer;
  bool _visible = true;
  bool _pointerOnDock = false;
  bool _dockFocused = false;
  int _popupCount = 0;

  bool get visible => _visible;
  bool get pointerOnDock => _pointerOnDock;
  bool get dockFocused => _dockFocused;
  int get popupCount => _popupCount;
  bool get isHeld => _pointerOnDock || _dockFocused || _popupCount > 0;

  void ping() {
    _visible = true;
    _resetHideTimer();
    notifyListeners();
  }

  void onExit() {
    if (!isHeld) {
      _cancelHideTimer();
      _visible = false;
      notifyListeners();
    }
  }

  void setPointerOnDock(bool onDock) {
    if (_pointerOnDock == onDock) return;
    _pointerOnDock = onDock;
    if (onDock) {
      _cancelHideTimer();
      _visible = true;
    } else {
      _resetHideTimer();
    }
    notifyListeners();
  }

  void setDockFocused(bool focused) {
    if (_dockFocused == focused) return;
    _dockFocused = focused;
    if (focused) {
      _cancelHideTimer();
      _visible = true;
    } else {
      _resetHideTimer();
    }
    notifyListeners();
  }

  void hold() {
    _popupCount++;
    _cancelHideTimer();
    _visible = true;
    notifyListeners();
  }

  void release() {
    if (_popupCount > 0) {
      _popupCount--;
      if (!isHeld) {
        _resetHideTimer();
      }
      notifyListeners();
    }
  }

  void _resetHideTimer() {
    _cancelHideTimer();
    _hideTimer = Timer(hideDelay, () {
      if (!isHeld) {
        _visible = false;
        notifyListeners();
      }
    });
  }

  void _cancelHideTimer() {
    _hideTimer?.cancel();
    _hideTimer = null;
  }

  @override
  void dispose() {
    _cancelHideTimer();
    super.dispose();
  }
}

class VoiceDockActivity extends InheritedNotifier<VoiceDockActivityNotifier> {
  const VoiceDockActivity({
    super.key,
    required VoiceDockActivityNotifier notifier,
    required super.child,
  }) : super(notifier: notifier);

  static VoiceDockActivityNotifier? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<VoiceDockActivity>()?.notifier;

  static VoiceDockActivityNotifier? maybeOf(BuildContext context) =>
      context.getInheritedWidgetOfExactType<VoiceDockActivity>()?.notifier;
}

/// Hosts [VoiceDockActivity] with a [MouseRegion] covering its child.
class VoiceDockActivityScope extends StatefulWidget {
  const VoiceDockActivityScope({
    super.key,
    required this.child,
    this.hideDelay = const Duration(milliseconds: 2500),
  });

  final Widget child;
  final Duration hideDelay;

  @override
  State<VoiceDockActivityScope> createState() => _VoiceDockActivityScopeState();
}

class _VoiceDockActivityScopeState extends State<VoiceDockActivityScope> {
  late final VoiceDockActivityNotifier _activity = VoiceDockActivityNotifier(
    hideDelay: widget.hideDelay,
  );

  @override
  void dispose() {
    _activity.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return VoiceDockActivity(
      notifier: _activity,
      child: Listener(
        behavior: HitTestBehavior.translucent,
        onPointerDown: (_) => _activity.ping(),
        child: MouseRegion(
          onEnter: (_) => _activity.ping(),
          onHover: (_) => _activity.ping(),
          onExit: (event) {
            final box = context.findRenderObject() as RenderBox?;
            if (box != null && box.attached) {
              final rect = box.localToGlobal(Offset.zero) & box.size;
              if (rect.contains(event.position)) {
                return;
              }
            }
            _activity.onExit();
          },
          child: widget.child,
        ),
      ),
    );
  }
}

/// The voice dock (§5.14): floats inside the voice view, 14 above its bottom edge.
///
/// Hides after 2.5 s of inactivity unless held by hover, focus, or open popup.
class VoiceDock extends ConsumerStatefulWidget {
  const VoiceDock({super.key, required this.channel, this.chat = false});

  final ChannelRef channel;
  final bool chat;

  @override
  ConsumerState<VoiceDock> createState() => _VoiceDockState();
}

class _VoiceDockState extends ConsumerState<VoiceDock>
    with TickerProviderStateMixin {
  late final AnimationController _translateController;
  late final AnimationController _opacityController;
  VoiceDockActivityNotifier? _activity;
  bool _lastVisible = true;

  @override
  void initState() {
    super.initState();
    _translateController = AnimationController.unbounded(
      vsync: this,
      value: 0.0,
    );
    _opacityController = AnimationController(vsync: this, value: 1.0);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final newActivity = VoiceDockActivity.of(context);
    if (newActivity != _activity) {
      _activity?.removeListener(_onActivityChanged);
      _activity = newActivity;
      _activity?.addListener(_onActivityChanged);
      if (_activity != null) {
        _lastVisible = _activity!.visible;
      }
    }
  }

  void _onActivityChanged() {
    final visible = _activity?.visible ?? true;
    if (visible == _lastVisible) return;
    _lastVisible = visible;
    final motion = OcMotion.of(context);
    final reduced = MediaQuery.disableAnimationsOf(context);

    if (reduced) {
      _translateController.value = visible ? 0.0 : 16.0;
      _opacityController.value = visible ? 1.0 : 0.0;
      setState(() {});
      return;
    }

    if (visible) {
      _translateController.animateWith(
        OcSpring.pop.simulation(
          _translateController.value,
          0.0,
          _translateController.velocity,
        ),
      );
      _opacityController.animateTo(
        1.0,
        duration: motion.fadeQuick,
        curve: OcMotion.curve,
      );
    } else {
      _translateController.animateTo(
        16.0,
        duration: motion.exit,
        curve: OcMotion.curveIn,
      );
      _opacityController.animateTo(
        0.0,
        duration: motion.exit,
        curve: OcMotion.curveIn,
      );
    }
  }

  @override
  void dispose() {
    _activity?.removeListener(_onActivityChanged);
    _translateController.dispose();
    _opacityController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final platform = Theme.of(context).platform;
    final session = ref.read(voiceSessionProvider.notifier);
    final voice = ref.watch(voiceSessionProvider);
    final capabilities = ref.watch(repositoryProvider).capabilities;
    final visible = _activity?.visible ?? true;

    return IgnorePointer(
      ignoring: !visible,
      child: AnimatedBuilder(
        animation: Listenable.merge([_translateController, _opacityController]),
        builder: (context, child) {
          final dy = _translateController.value;
          final opacity = _opacityController.value.clamp(0.0, 1.0);
          return Transform.translate(
            offset: Offset(0, dy),
            child: Opacity(
              opacity: opacity,
              child: child,
            ),
          );
        },
        child: MouseRegion(
          onEnter: (_) => _activity?.setPointerOnDock(true),
          onExit: (_) => _activity?.setPointerOnDock(false),
          child: Focus(
            onFocusChange: (f) => _activity?.setDockFocused(f),
            child: LayoutBuilder(
              builder: (context, constraints) {
                final maxW = math.max(0.0, constraints.maxWidth - 24);
                return Container(
                  key: ShellKeys.dock,
                  height: 64,
                  constraints: BoxConstraints(maxWidth: maxW),
                  padding: const EdgeInsets.all(7),
                  decoration: BoxDecoration(
                    color: colors.sidebar,
                    borderRadius: BorderRadius.circular(31),
                    border: Border.all(color: colors.border, width: 1),
                    boxShadow: OcShadows.menu(colors),
                  ),
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (widget.chat) ...[
                          VoiceChatButton(
                            channel: widget.channel,
                            round: true,
                          ),
                          const SizedBox(width: 10),
                          Container(width: 1, height: 28, color: colors.border),
                          const SizedBox(width: 10),
                        ],
                        _DockRoundButton(
                          icon: OcIcons.mic,
                          activeIcon: OcIcons.micOff,
                          tooltip: voice.muted ? 'Unmute' : 'Mute',
                          shortcut: appShortcut(
                            LogicalKeyboardKey.keyM,
                            platform,
                            shift: true,
                          ),
                          active: voice.muted,
                          onPressed: session.toggleMute,
                        ),
                        if (capabilities.camera) ...[
                          const SizedBox(width: 10),
                          Builder(
                            builder: (btnContext) => _DockRoundButton(
                              icon: OcIcons.videocamOff,
                              activeIcon: OcIcons.videocam,
                              tooltip: voice.camera
                                  ? 'Turn off camera'
                                  : 'Turn on camera',
                              active: voice.camera,
                              onPressed: () => toggleCamera(context, ref),
                              onSecondaryTap: (offset) async {
                                final activity = _activity;
                                activity?.hold();
                                try {
                                  await showCameraMenu(
                                    context,
                                    ref,
                                    offset,
                                  );
                                } finally {
                                  activity?.release();
                                }
                              },
                            ),
                          ),
                        ],
                        if (capabilities.screenShare) ...[
                          const SizedBox(width: 10),
                          Builder(
                            builder: (btnContext) => _DockRoundButton(
                              icon: OcIcons.screenShare,
                              tooltip: voice.screensharing
                                  ? 'Screen share options'
                                  : 'Share your screen',
                              active: voice.screensharing,
                              onPressed: () async {
                                final activity = _activity;
                                activity?.hold();
                                try {
                                  final box =
                                      btnContext.findRenderObject()
                                          as RenderBox?;
                                  final rect =
                                      box == null || !box.attached
                                          ? null
                                          : box.localToGlobal(Offset.zero) &
                                              box.size;
                                  await screenshareControl(
                                    context,
                                    ref,
                                    anchor: rect,
                                  );
                                } finally {
                                  activity?.release();
                                }
                              },
                            ),
                          ),
                        ],
                        if (capabilities.soundboard) ...[
                          const SizedBox(width: 10),
                          Builder(
                            builder: (btnContext) => _DockRoundButton(
                              icon: OcIcons.musicNote,
                              tooltip: 'Soundboard',
                              shortcut: appShortcut(
                                LogicalKeyboardKey.keyB,
                                platform,
                                shift: true,
                              ),
                              active: false,
                              onPressed: () async {
                                final activity = _activity;
                                activity?.hold();
                                try {
                                  final box =
                                      btnContext.findRenderObject()
                                          as RenderBox?;
                                  final rect =
                                      box == null || !box.attached
                                          ? Rect.zero
                                          : box.localToGlobal(Offset.zero) &
                                              box.size;
                                  await showSoundboardPicker(
                                    context,
                                    anchor: rect,
                                  );
                                } finally {
                                  activity?.release();
                                }
                              },
                            ),
                          ),
                        ],
                        const SizedBox(width: 10),
                        _DockRoundButton(
                          icon: OcIcons.headphones,
                          activeIcon: OcIcons.headsetOff,
                          tooltip: voice.deafened ? 'Undeafen' : 'Deafen',
                          shortcut: appShortcut(
                            LogicalKeyboardKey.keyD,
                            platform,
                            shift: true,
                          ),
                          active: voice.deafened,
                          onPressed: session.toggleDeafen,
                        ),
                        const SizedBox(width: 16),
                        _DisconnectPill(
                          onPressed: () => leaveVoice(context, ref),
                        ),
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      ),
    );
  }
}

class VoiceChatButton extends ConsumerWidget {
  const VoiceChatButton({super.key, required this.channel, this.round = false});

  final ChannelRef channel;
  final bool round;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final open = ref.watch(
      voiceViewPrefsProvider.select((prefs) => prefs.chatOpen),
    );
    final unread = ref.watch(
      activityProvider(
        channel.server,
      ).select((activity) => activity.of(channel.channel).read.unread),
    );
    final toggle = ref.read(voiceViewPrefsProvider.notifier).toggleChat;
    final tooltip = switch ((open, unread)) {
      (true, _) => 'Hide chat',
      (false, 0) => 'Show chat',
      (false, final count) => 'Show chat, $count unread',
    };
    final button = round
        ? _DockRoundButton(
            icon: OcIcons.chat,
            tooltip: tooltip,
            active: open,
            onPressed: toggle,
          )
        : OcIconButton(
            icon: OcIcons.chat,
            tooltip: tooltip,
            active: open,
            onPressed: toggle,
          );
    return Stack(
      clipBehavior: Clip.none,
      children: [
        button,
        if (!open && unread > 0)
          Positioned(
            right: 0,
            top: 0,
            child: IgnorePointer(child: UnreadBadge(count: unread)),
          ),
      ],
    );
  }
}

class _DockRoundButton extends StatelessWidget {
  const _DockRoundButton({
    required this.icon,
    this.activeIcon,
    required this.tooltip,
    this.shortcut,
    this.active = false,
    required this.onPressed,
    this.onSecondaryTap,
  });

  final IconData icon;
  final IconData? activeIcon;
  final String tooltip;
  final SingleActivator? shortcut;
  final bool active;
  final VoidCallback? onPressed;
  final ValueChanged<Offset>? onSecondaryTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final message = shortcut == null
        ? tooltip
        : '$tooltip (${shortcutLabel(shortcut!, Theme.of(context).platform)})';

    final button = Hoverable(
      onTap: onPressed,
      onSecondaryTap: onSecondaryTap,
      semanticLabel: tooltip,
      button: true,
      selected: active ? true : null,
      focusRadius: BorderRadius.circular(24),
      builder: (context, state) {
        final background = active
            ? (state.active
                ? Color.lerp(colors.accent, colors.onAccent, 0.12)!
                : colors.accent)
            : (state.active ? colors.selected : colors.surface);
        final borderColor = active
            ? colors.accent
            : (state.active ? colors.selected : colors.border);
        final foreground = active ? colors.onAccent : colors.text;

        final circle = AnimatedContainer(
          duration: motion.hover,
          curve: OcMotion.curve,
          width: 48,
          height: 48,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: background,
            border: Border.all(color: borderColor, width: 1),
          ),
          child: Center(
            child: Icon(
              active ? (activeIcon ?? icon) : icon,
              size: 20,
              color: foreground,
            ),
          ),
        );

        return AnimatedScale(
          scale: state.pressed ? 0.97 : 1.0,
          duration: motion.press,
          curve: OcMotion.curve,
          child: circle,
        );
      },
    );

    return Tooltip(
      message: message,
      excludeFromSemantics: true,
      child: button,
    );
  }
}

class _DisconnectPill extends StatelessWidget {
  const _DisconnectPill({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    return Hoverable(
      onTap: onPressed,
      semanticLabel: 'Disconnect',
      button: true,
      focusRadius: BorderRadius.circular(24),
      builder: (context, state) {
        final background = state.active
            ? Color.lerp(colors.accent, colors.onAccent, 0.12)!
            : colors.accent;
        return AnimatedScale(
          scale: state.pressed ? 0.97 : 1.0,
          duration: motion.press,
          curve: OcMotion.curve,
          child: AnimatedContainer(
            duration: motion.hover,
            curve: OcMotion.curve,
            height: 48,
            padding: const EdgeInsets.fromLTRB(14, 0, 18, 0),
            decoration: BoxDecoration(
              color: background,
              borderRadius: BorderRadius.circular(24),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(OcIcons.callEnd, size: 20, color: colors.onAccent),
                const SizedBox(width: 8),
                Text(
                  'Disconnect',
                  style: OcText.header.copyWith(
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                    color: colors.onAccent,
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
