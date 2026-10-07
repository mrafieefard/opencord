import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/channel_sidebar.dart';
import 'package:opencord/features/chat/chat_area.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/shell/debug_menu.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_layout.dart';
import 'package:opencord/features/shell/shortcuts.dart';
import 'package:opencord/features/shell/window_title.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

/// The desktop window: rail, channel sidebar, main column and member panel
/// (§3), with the responsive rules and the app-wide shortcuts (§7).
class DesktopShell extends ConsumerStatefulWidget {
  const DesktopShell({super.key});

  static const railKey = Key('shell-rail');
  static const sidebarKey = Key('shell-sidebar');
  static const mainKey = Key('shell-main');
  static const membersKey = Key('shell-members');

  @override
  ConsumerState<DesktopShell> createState() => _DesktopShellState();
}

class _DesktopShellState extends ConsumerState<DesktopShell> {
  bool _sidebarDrawer = false;
  bool _membersDrawer = false;
  ShellLayout _layout = ShellLayout.wide;

  @override
  void initState() {
    super.initState();
    ref.listenManual(
      windowTitleProvider,
      (previous, title) => ref.read(nativeWindowProvider).setTitle(title),
      fireImmediately: true,
    );
  }

  void _toggleFullscreen() {
    final fullscreen = ref.read(windowStatusProvider).fullscreen;
    ref.read(nativeWindowProvider).setFullscreen(!fullscreen);
  }

  void _toggleMembers() {
    if (_layout.membersInline) {
      ref.read(memberPanelProvider.notifier).toggle();
    } else {
      setState(() => _membersDrawer = !_membersDrawer);
    }
  }

  void _stepChannel(int delta, {bool unreadOnly = false}) {
    final server = ref.read(currentServerProvider);
    if (server == null) return;
    final channels = ref.read(serverProvider(server)).data?.channels;
    if (channels == null) return;
    final order = [
      for (final channel in navigableChannels(channels.values)) channel.id,
    ];
    final current = ref.read(currentChannelProvider);
    final activity = ref.read(activityProvider(server));
    final next = unreadOnly
        ? neighborWhere(order, current, delta, (id) => activity.of(id).unread)
        : neighbor(order, current, delta);
    if (next != null) {
      ref.read(navigationProvider.notifier).openChannel(server, next);
    }
  }

  void _stepServer(int delta) {
    final order = [
      for (final server in ref.read(serverListProvider)) server.key,
    ];
    final next = neighbor(order, ref.read(currentServerProvider), delta);
    if (next != null) ref.read(navigationProvider.notifier).openServer(next);
  }

  void _markRead() {
    final server = ref.read(currentServerProvider);
    final channel = ref.read(currentChannelProvider);
    if (server != null && channel != null) {
      ref.read(activityProvider(server).notifier).markRead(channel);
    }
  }

  Map<Type, Action<Intent>> _actions() => {
    ChannelStepIntent: CallbackAction<ChannelStepIntent>(
      onInvoke: (intent) => _stepChannel(intent.delta),
    ),
    UnreadStepIntent: CallbackAction<UnreadStepIntent>(
      onInvoke: (intent) => _stepChannel(intent.delta, unreadOnly: true),
    ),
    ServerStepIntent: CallbackAction<ServerStepIntent>(
      onInvoke: (intent) => _stepServer(intent.delta),
    ),
    ToggleMuteIntent: CallbackAction<ToggleMuteIntent>(
      onInvoke: (_) => ref.read(voiceSessionProvider.notifier).toggleMute(),
    ),
    ToggleDeafenIntent: CallbackAction<ToggleDeafenIntent>(
      onInvoke: (_) => ref.read(voiceSessionProvider.notifier).toggleDeafen(),
    ),
    ToggleMembersIntent: CallbackAction<ToggleMembersIntent>(
      onInvoke: (_) => _toggleMembers(),
    ),
    MarkReadIntent: CallbackAction<MarkReadIntent>(
      onInvoke: (_) => _markRead(),
    ),
    DebugMenuIntent: CallbackAction<DebugMenuIntent>(
      onInvoke: (_) => showDebugMenu(context, ref),
    ),
    FullscreenIntent: CallbackAction<FullscreenIntent>(
      onInvoke: (_) => _toggleFullscreen(),
    ),
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final membersShown = ref.watch(memberPanelProvider);
    return Shortcuts(
      shortcuts: shellShortcuts(Theme.of(context).platform),
      child: Actions(
        actions: _actions(),
        child: Focus(
          autofocus: true,
          child: LayoutBuilder(
            builder: (context, constraints) {
              final layout = ShellLayout.forWidth(constraints.maxWidth);
              _layout = layout;
              final border = VerticalDivider(
                width: 1,
                thickness: 1,
                color: colors.border,
              );
              final membersInline = layout.membersInline && membersShown;
              return Stack(
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      const SizedBox(
                        key: DesktopShell.railKey,
                        width: OcSize.railWidth,
                        child: ServerRail(),
                      ),
                      border,
                      if (layout.sidebarInline) ...[
                        SizedBox(
                          key: DesktopShell.sidebarKey,
                          width: layout.sidebarWidth,
                          child: const ChannelSidebar(leadingControls: true),
                        ),
                        border,
                      ],
                      Expanded(
                        key: DesktopShell.mainKey,
                        child: ChatArea(
                          membersShown: layout.membersInline
                              ? membersShown
                              : _membersDrawer,
                          onToggleMembers: _toggleMembers,
                          onOpenSidebar: layout.sidebarInline
                              ? null
                              : () => setState(() => _sidebarDrawer = true),
                          leadingControls: !layout.sidebarInline,
                          trailingControls: !membersInline,
                        ),
                      ),
                      if (membersInline) ...[
                        border,
                        SizedBox(
                          key: DesktopShell.membersKey,
                          width: OcSize.memberPanelWidth,
                          child: MemberPanel(
                            onClose: _toggleMembers,
                            trailingControls: true,
                          ),
                        ),
                      ],
                    ],
                  ),
                  if (!layout.sidebarInline && _sidebarDrawer)
                    _Drawer(
                      fromRight: false,
                      inset: OcSize.railWidth + 1,
                      onClose: () => setState(() => _sidebarDrawer = false),
                      child: SizedBox(
                        key: DesktopShell.sidebarKey,
                        width: layout.sidebarWidth,
                        child: const ChannelSidebar(),
                      ),
                    ),
                  if (!layout.membersInline && _membersDrawer)
                    _Drawer(
                      fromRight: true,
                      onClose: () => setState(() => _membersDrawer = false),
                      child: SizedBox(
                        key: DesktopShell.membersKey,
                        width: OcSize.memberPanelWidth,
                        child: MemberPanel(
                          onClose: () => setState(() => _membersDrawer = false),
                        ),
                      ),
                    ),
                ],
              );
            },
          ),
        ),
      ),
    );
  }
}

/// A panel over the layout with a scrim behind it. Escape or a click on
/// the scrim closes it.
class _Drawer extends StatefulWidget {
  const _Drawer({
    required this.fromRight,
    required this.onClose,
    required this.child,
    this.inset = 0,
  });

  final bool fromRight;
  final VoidCallback onClose;
  final Widget child;

  /// Space left uncovered on the opening side (the rail stays usable).
  final double inset;

  @override
  State<_Drawer> createState() => _DrawerState();
}

class _DrawerState extends State<_Drawer> {
  final _scope = FocusScopeNode(debugLabel: 'drawer');

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _scope.requestFocus();
    });
  }

  @override
  void dispose() {
    _scope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final fromRight = widget.fromRight;
    final onClose = widget.onClose;
    return Positioned.fill(
      left: fromRight ? 0 : widget.inset,
      child: Shortcuts(
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
        },
        child: Actions(
          actions: {
            DismissIntent: CallbackAction<DismissIntent>(
              onInvoke: (_) => onClose(),
            ),
          },
          child: FocusScope(
            node: _scope,
            child: Stack(
              children: [
                Positioned.fill(
                  child: GestureDetector(
                    onTap: onClose,
                    child: ColoredBox(
                      color: colors.scrim.withValues(
                        alpha: colors.scrim.a * 0.6,
                      ),
                    ),
                  ),
                ),
                Align(
                  alignment: fromRight
                      ? Alignment.centerRight
                      : Alignment.centerLeft,
                  child: TweenAnimationBuilder<double>(
                    tween: Tween(begin: 1, end: 0),
                    duration: motion.morph,
                    curve: OcMotion.curve,
                    builder: (context, offset, child) => FractionalTranslation(
                      translation: Offset(fromRight ? offset : -offset, 0),
                      child: child,
                    ),
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        border: Border(
                          left: fromRight
                              ? BorderSide(color: colors.border)
                              : BorderSide.none,
                          right: fromRight
                              ? BorderSide.none
                              : BorderSide(color: colors.border),
                        ),
                      ),
                      child: widget.child,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
