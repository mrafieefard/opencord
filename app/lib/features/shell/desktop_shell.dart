import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/voice_view_prefs.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/providers/voice_cues.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/channel_sidebar.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/chat/attachment_drop.dart';
import 'package:opencord/features/chat/chat_area.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/dialogs/identity_changed_dialog.dart';
import 'package:opencord/features/home/home_main.dart';
import 'package:opencord/features/home/home_sidebar.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/music/music_access.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/servers/server_rail.dart';
import 'package:opencord/features/shell/debug_menu.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/navigation_history.dart';
import 'package:opencord/features/shell/pane_button.dart';
import 'package:opencord/features/shell/pane_switch.dart';
import 'package:opencord/features/shell/shell_drawer.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/shell_layout.dart';
import 'package:opencord/features/shell/shell_splitter.dart';
import 'package:opencord/features/shell/shortcuts.dart';
import 'package:opencord/features/shell/side_slot.dart';
import 'package:opencord/features/shell/title_crumb.dart';
import 'package:opencord/features/shell/window_title.dart';
import 'package:opencord/features/settings/user_settings.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/switcher/quick_switcher.dart';
import 'package:opencord/features/window/app_title_bar.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/features/voice/voice_reconnect_bar.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/shell/mac_menu_bar.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/motion/spring_value.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/widgets/notched_float.dart';
import 'package:opencord/ui/widgets/oc_float.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The desktop window: rail, channel sidebar, main column and member panel
/// (§3), with the responsive rules and the app-wide shortcuts (§7).
class DesktopShell extends ConsumerStatefulWidget {
  const DesktopShell({super.key});

  static const railKey = ShellKeys.rail;
  static const sidebarKey = ShellKeys.left;
  static const mainKey = ShellKeys.main;
  static const membersKey = ShellKeys.panel;

  @override
  ConsumerState<DesktopShell> createState() => _DesktopShellState();
}

class _DesktopShellState extends ConsumerState<DesktopShell>
    with TickerProviderStateMixin {
  bool _sidebarDrawer = false;
  bool _membersDrawer = false;

  /// [_membersDrawer] is on its way out: the page's notch is already opening.
  bool _membersDrawerClosing = false;
  final _sidebarDrawerKey = GlobalKey<ShellDrawerState>();
  final _membersDrawerKey = GlobalKey<ShellDrawerState>();
  ShellLayout _layout = ShellLayout.wide;

  late final AnimationController _sidebarWidthController =
      AnimationController.unbounded(vsync: this);
  bool _isDraggingSplitter = false;
  double? _splitterDragWidth;
  double? _lastClampedTarget;

  @override
  void dispose() {
    _sidebarWidthController.dispose();
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    ref.listenManual(
      windowTitleProvider,
      (previous, title) => ref.read(nativeWindowProvider).setTitle(title),
      fireImmediately: true,
    );
    ref.listenManual(identityChangesProvider, (previous, next) {
      for (final MapEntry(key: server, value: status) in next.entries) {
        if (previous?.containsKey(server) ?? false) continue;
        final name =
            ref.read(serverProvider(server)).data?.info.name ??
            ref
                .read(serverListProvider)
                .where((summary) => summary.key == server)
                .firstOrNull
                ?.name ??
            server;
        showIdentityChanged(
          context,
          serverKey: server,
          name: name,
          status: status,
        );
      }
    });
    ref.listenManual(appLinkInboxProvider, (_, links) {
      if (links.isNotEmpty) _openLinks();
    }, fireImmediately: true);
    // Voice sounds play while the shell is up (Phase 2 §12).
    ref.listenManual(voiceCuesProvider, (_, _) {});
    // Whatever asks for Music opens the pane too (§5.9), `/music` for one.
    ref.listenManual(rightPanelModeProvider, (_, mode) {
      if (mode == RightPanelMode.music && mounted) _openRightPanel();
    });
    // Going anywhere shuts the drawers, as the prototype's `go` does.
    ref.listenManual(navigationProvider, (_, _) {
      _sidebarDrawerKey.currentState?.close();
      _membersDrawerKey.currentState?.close();
    });
    ref.listenManual(audioNoticeProvider, (_, notice) {
      if (notice != null && mounted) {
        showOcToast(
          context,
          notice.message,
          actionLabel: notice.actionLabel,
          onAction: notice.onAction,
        );
      }
    });
  }

  /// Opens the `opencord://` links waiting in the inbox (§15), after the
  /// frame, since they may open dialogs.
  void _openLinks() {
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      if (!mounted) return;
      for (final text in ref.read(appLinkInboxProvider.notifier).takeAll()) {
        final link = parseAppLink(text);
        if (link == null) {
          showOcToast(context, 'Opencord cannot open that link.');
          continue;
        }
        await openAppLink(context, ref, link);
        if (!mounted) return;
      }
    });
  }

  void _toggleFullscreen() {
    final fullscreen = ref.read(windowStatusProvider).fullscreen;
    ref.read(nativeWindowProvider).setFullscreen(!fullscreen);
  }

  void _selectMembers() => _showRightPanel(RightPanelMode.people);

  /// Opens the right-side panel on [mode].
  void _showRightPanel(RightPanelMode mode) {
    ref.read(rightPanelModeProvider.notifier).set(mode);
    _openRightPanel();
  }

  /// A voice channel's chat holds the panel's place while it is open, so it
  /// closes. A drawer on its way out turns back.
  void _openRightPanel() {
    if (_layout.membersInline) {
      if (ref.read(voiceChatShownProvider)) {
        ref.read(voiceViewPrefsProvider.notifier).toggleChat();
      }
      ref.read(memberPanelProvider.notifier).open();
    } else {
      _membersDrawerKey.currentState?.reopen();
      setState(() {
        _membersDrawer = true;
        _membersDrawerClosing = false;
      });
    }
  }

  void _closeRightPanel() {
    if (_layout.membersInline) {
      ref.read(memberPanelProvider.notifier).close();
    } else {
      _membersDrawerKey.currentState?.close();
    }
  }

  /// The users list, or the music panel in its place (§5.13). What's under
  /// the new one's header fades in, and the old one goes at once.
  Widget _rightPanel({required bool music}) {
    final channel = ref.read(musicChannelProvider);
    return AnimatedSwitcher(
      duration: OcMotion.of(context).fade,
      switchInCurve: OcMotion.curve,
      layoutBuilder: (current, _) => current ?? const SizedBox.shrink(),
      transitionBuilder: (child, animation) =>
          PaneSwapFade(opacity: animation, child: child),
      child: music && channel != null
          ? MusicPanel(
              key: const ValueKey('music'),
              channel: channel,
              onClose: _closeRightPanel,
            )
          : MemberPanel(
              key: const ValueKey('people'),
              onClose: _closeRightPanel,
            ),
    );
  }

  void _toggleMembers() {
    final panelOpen = _layout.membersInline
        ? ref.read(memberPanelProvider) &&
              ref.read(serverListProvider).isNotEmpty &&
              !ref.read(voiceChatShownProvider)
        : _membersDrawer && !_membersDrawerClosing;
    final mode = ref.read(rightPanelModeProvider);
    if (panelOpen && mode == RightPanelMode.people) {
      _closeRightPanel();
    } else {
      _selectMembers();
    }
  }

  void _stepChannel(int delta, {bool unreadOnly = false}) {
    if (ref.read(homeShownProvider)) return;
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

  /// Escape with nothing else to close: leaves voice focus mode, or marks
  /// the open channel read (§7).
  void _escape() {
    final server = ref.read(currentServerProvider);
    final channel = ref.read(currentChannelProvider);
    if (server == null || channel == null) return;
    final focus = voiceFocusProvider((server: server, channel: channel));
    if (ref.read(focus) != null) {
      ref.read(focus.notifier).clear();
      return;
    }
    ref.read(activityProvider(server).notifier).markRead(channel);
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
    OpenSoundboardIntent: CallbackAction<OpenSoundboardIntent>(
      onInvoke: (_) => _openSoundboard(),
    ),
    MarkReadIntent: CallbackAction<MarkReadIntent>(onInvoke: (_) => _escape()),
    UserSettingsIntent: CallbackAction<UserSettingsIntent>(
      onInvoke: (_) => showUserSettings(context),
    ),
    QuickSwitcherIntent: CallbackAction<QuickSwitcherIntent>(
      onInvoke: (_) => showQuickSwitcher(context, ref),
    ),
    DebugMenuIntent: CallbackAction<DebugMenuIntent>(
      onInvoke: (_) => showDebugMenu(context, ref),
    ),
    FullscreenIntent: CallbackAction<FullscreenIntent>(
      onInvoke: (_) => _toggleFullscreen(),
    ),
    BackIntent: CallbackAction<BackIntent>(
      onInvoke: (_) => ref.read(navigationHistoryProvider.notifier).back(),
    ),
    ForwardIntent: CallbackAction<ForwardIntent>(
      onInvoke: (_) => ref.read(navigationHistoryProvider.notifier).forward(),
    ),
  };

  /// From the keyboard there is no button to open it from: it opens above
  /// the window's bottom middle.
  void _openSoundboard() {
    final voice = ref.read(voiceSessionProvider);
    if (!voice.connected ||
        !ref.read(repositoryProvider).capabilities.soundboard) {
      return;
    }
    final size = MediaQuery.sizeOf(context);
    showSoundboardPicker(
      context,
      anchor: Rect.fromCenter(
        center: Offset(size.width / 2, size.height - OcSpace.s24),
        width: 1,
        height: 1,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final serverView =
        !ref.watch(homeShownProvider) &&
        ref.watch(serverListProvider.select((servers) => servers.isNotEmpty));
    final paneWanted =
        ref.watch(memberPanelProvider) && !ref.watch(voiceChatShownProvider);

    // The voice channel open, or else the one this device is in: the
    // now-playing card opens its music from a text channel too (§19.3).
    final musicChannel = ref.watch(musicChannelProvider);
    final musicShown =
        musicChannel != null &&
        ref.watch(musicOfferedProvider(musicChannel)) &&
        ref.watch(rightPanelModeProvider) == RightPanelMode.music;
    final paneWidth = musicShown ? OcFloats.sideMusic : OcFloats.side;

    // Text fields (the composer, searches) need Material underneath.
    return Material(
      type: MaterialType.transparency,
      child: Shortcuts(
        shortcuts: leavingKeybinds(
          shellShortcuts(Theme.of(context).platform),
          ref.watch(keybindsProvider.select((keybinds) => keybinds.bound)),
        ),
        child: Actions(
          actions: _actions(),
          child: MacMenuBar(
            child: Listener(
              onPointerDown: (event) {
                if (event.buttons & kBackMouseButton != 0) {
                  ref.read(navigationHistoryProvider.notifier).back();
                } else if (event.buttons & kForwardMouseButton != 0) {
                  ref.read(navigationHistoryProvider.notifier).forward();
                }
              },
              child: Focus(
                autofocus: true,
                child: ColoredBox(
                  color: colors.rail,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      const AppTitleBar(
                        leading: HistoryArrows(),
                        center: TitleCrumb(),
                      ),
                      const VoiceReconnectBar(),
                      Expanded(
                        child: LayoutBuilder(
                          builder: (context, constraints) {
                            final layout = ShellLayout.forWidth(
                              constraints.maxWidth,
                            );
                            _layout = layout;
                            final defaultWidth = layout.sidebarWidth;
                            final savedWidth = ref.watch(sidebarWidthProvider);
                            final targetWidth = savedWidth ?? defaultWidth;

                            final paneOpen = layout.membersInline
                                ? paneWanted
                                : _membersDrawer && !_membersDrawerClosing;
                            final slotOpen =
                                layout.membersInline &&
                                paneWanted &&
                                serverView;

                            final minWidth = OcFloats.leftMin;
                            final maxWidth = math.max(
                              minWidth,
                              math.min(
                                OcFloats.leftMax,
                                constraints.maxWidth -
                                    OcFloats.rail -
                                    4 * OcFloats.gap -
                                    (slotOpen ? paneWidth : 0) -
                                    380.0,
                              ),
                            );
                            final clampedTarget = targetWidth.clamp(
                              minWidth,
                              maxWidth,
                            );

                            if (!_isDraggingSplitter) {
                              if (_lastClampedTarget == null) {
                                _sidebarWidthController.value = clampedTarget;
                                _lastClampedTarget = clampedTarget;
                              } else if (_lastClampedTarget != clampedTarget) {
                                _lastClampedTarget = clampedTarget;
                                if (MediaQuery.disableAnimationsOf(context)) {
                                  _sidebarWidthController.value = clampedTarget;
                                } else {
                                  _sidebarWidthController.animateWith(
                                    OcSpring.pane.simulation(
                                      _sidebarWidthController.value,
                                      clampedTarget,
                                      _sidebarWidthController.velocity,
                                    ),
                                  );
                                }
                              }
                            }

                            return AnimatedBuilder(
                              animation: _sidebarWidthController,
                              builder: (context, _) {
                                final currentSidebarWidth = _isDraggingSplitter
                                    ? (_splitterDragWidth ?? clampedTarget)
                                    : _sidebarWidthController.value;

                                return SpringValue.listenable(
                                  target: paneOpen ? 0 : OcFloats.notch,
                                  spring: OcSpring.pane,
                                  falling: OcSpring.paneOut,
                                  fallingDelay: OcMotion.notchDelay,
                                  builder: (context, notch, _) => ShellMetrics(
                                    mainInner: ShellMetrics.mainInnerFor(
                                      window: constraints.maxWidth,
                                      left: layout.sidebarInline
                                          ? currentSidebarWidth
                                          : 0,
                                      side: slotOpen ? paneWidth : 0,
                                    ),
                                    // Only a server's page has a notch, and it
                                    // goes and comes with the page.
                                    notch: serverView
                                        ? notch
                                        : const AlwaysStoppedAnimation(0.0),
                                    layout: layout,
                                    child: Stack(
                                      children: [
                                        Padding(
                                          padding: const EdgeInsets.fromLTRB(
                                            OcFloats.gap,
                                            0,
                                            OcFloats.gap,
                                            OcFloats.gap,
                                          ),
                                          child: Row(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.stretch,
                                            children: [
                                              const SizedBox(
                                                width: OcSize.railWidth,
                                                child: OcFloat(
                                                  key: ShellKeys.rail,
                                                  child: ServerRail(),
                                                ),
                                              ),
                                              if (layout.sidebarInline) ...[
                                                const SizedBox(
                                                  width: OcFloats.gap,
                                                ),
                                                SizedBox(
                                                  key: ShellKeys.left,
                                                  width: currentSidebarWidth,
                                                  child: const _LeftColumn(),
                                                ),
                                                ShellSplitter(
                                                  key: ShellKeys.splitter,
                                                  currentWidth:
                                                      currentSidebarWidth,
                                                  minWidth: minWidth,
                                                  maxWidth: maxWidth,
                                                  defaultWidth: defaultWidth,
                                                  isDragging:
                                                      _isDraggingSplitter,
                                                  onDragStart: () {
                                                    _sidebarWidthController
                                                        .stop();
                                                    setState(() {
                                                      _isDraggingSplitter =
                                                          true;
                                                      _splitterDragWidth =
                                                          currentSidebarWidth
                                                              .clamp(
                                                                minWidth,
                                                                maxWidth,
                                                              );
                                                    });
                                                  },
                                                  onDragDelta: (dx) {
                                                    setState(() {
                                                      _splitterDragWidth =
                                                          ((_splitterDragWidth ??
                                                                      currentSidebarWidth) +
                                                                  dx)
                                                              .clamp(
                                                                minWidth,
                                                                maxWidth,
                                                              );
                                                      _sidebarWidthController
                                                              .value =
                                                          _splitterDragWidth!;
                                                    });
                                                  },
                                                  onDragEnd: () {
                                                    if (!_isDraggingSplitter) {
                                                      return;
                                                    }
                                                    final finalWidth =
                                                        _splitterDragWidth ??
                                                        _sidebarWidthController
                                                            .value;
                                                    setState(() {
                                                      _isDraggingSplitter =
                                                          false;
                                                      _splitterDragWidth = null;
                                                      _lastClampedTarget =
                                                          finalWidth;
                                                    });
                                                    ref
                                                        .read(
                                                          sidebarWidthProvider
                                                              .notifier,
                                                        )
                                                        .save(finalWidth);
                                                  },
                                                  onReset: () {
                                                    ref
                                                        .read(
                                                          sidebarWidthProvider
                                                              .notifier,
                                                        )
                                                        .reset();
                                                  },
                                                  onStep: (delta) {
                                                    final next =
                                                        (currentSidebarWidth +
                                                                delta)
                                                            .clamp(
                                                              minWidth,
                                                              maxWidth,
                                                            );
                                                    _sidebarWidthController
                                                            .value =
                                                        next;
                                                    _lastClampedTarget = next;
                                                    ref
                                                        .read(
                                                          sidebarWidthProvider
                                                              .notifier,
                                                        )
                                                        .save(next);
                                                    setState(() {});
                                                  },
                                                ),
                                              ] else ...[
                                                const SizedBox(
                                                  width: OcFloats.gap,
                                                ),
                                              ],
                                              Expanded(
                                                child: _MainColumn(
                                                  layout: layout,
                                                  paneOpen: paneOpen,
                                                  onOpenPane: _openRightPanel,
                                                  onOpenSidebar:
                                                      layout.sidebarInline
                                                      ? null
                                                      : () => setState(
                                                          () => _sidebarDrawer =
                                                              true,
                                                        ),
                                                ),
                                              ),
                                              if (layout.membersInline)
                                                SideSlot(
                                                  open: slotOpen,
                                                  width: paneWidth,
                                                  child: _rightPanel(
                                                    music: musicShown,
                                                  ),
                                                ),
                                            ],
                                          ),
                                        ),
                                        if (!layout.sidebarInline &&
                                            _sidebarDrawer)
                                          ShellDrawer(
                                            key: _sidebarDrawerKey,
                                            fromRight: false,
                                            inset:
                                                OcFloats.gap * 2 +
                                                OcFloats.rail,
                                            width: layout.leftDrawerWidth(
                                              constraints.maxWidth,
                                            ),
                                            onClose: () => setState(
                                              () => _sidebarDrawer = false,
                                            ),
                                            child: const _LeftColumn(),
                                          ),
                                        if (!layout.membersInline &&
                                            _membersDrawer)
                                          ShellDrawer(
                                            key: _membersDrawerKey,
                                            fromRight: true,
                                            width: paneWidth,
                                            onClosing: () => setState(
                                              () =>
                                                  _membersDrawerClosing = true,
                                            ),
                                            onClose: () => setState(() {
                                              _membersDrawer = false;
                                              _membersDrawerClosing = false;
                                            }),
                                            child: _rightPanel(
                                              music: musicShown,
                                            ),
                                          ),
                                      ],
                                    ),
                                  ),
                                );
                              },
                            );
                          },
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The left column: head slot, channels float, voice panel and user panel (§2).
class _LeftColumn extends ConsumerWidget {
  const _LeftColumn();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final homeShown = ref.watch(homeShownProvider);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (homeShown) const HomeHead() else const ServerHead(),
        const SizedBox(height: OcFloats.gap),
        Expanded(
          child: OcFloat(
            key: ShellKeys.channels,
            color: colors.sidebar,
            child: homeShown ? const HomeSidebar() : const ChannelSidebar(),
          ),
        ),
        const _VoiceCardSlot(),
        const SizedBox(height: OcFloats.gap),
        ConstrainedBox(
          constraints: const BoxConstraints(minHeight: OcFloats.bottom),
          child: const OcFloat(key: ShellKeys.me, child: UserPanel()),
        ),
      ],
    );
  }
}

/// The animated slot for the voice connected card (§5.5).
///
/// Springs into view on [OcSpring.pane] and leaves on [OcSpring.paneOut].
/// Keeps showing the last channel name while leaving so nothing jumps.
class _VoiceCardSlot extends ConsumerStatefulWidget {
  const _VoiceCardSlot();

  @override
  ConsumerState<_VoiceCardSlot> createState() => _VoiceCardSlotState();
}

class _VoiceCardSlotState extends ConsumerState<_VoiceCardSlot> {
  String? _lastServer;
  int? _lastChannel;

  @override
  Widget build(BuildContext context) {
    final voice = ref.watch(voiceSessionProvider);
    final inVoice = voice.serverKey != null && voice.channelId != null;
    if (inVoice) {
      _lastServer = voice.serverKey;
      _lastChannel = voice.channelId;
    }

    final colors = context.oc;
    final textScaler = MediaQuery.textScalerOf(context);
    final scale = textScaler.scale(1.0);
    final cardHeight = 103.4 + math.max(0.0, (scale - 1.0) * 32.0);

    return SpringValue(
      target: inVoice ? 1.0 : 0.0,
      spring: OcSpring.pane,
      falling: OcSpring.paneOut,
      builder: (context, factor, child) {
        if ((factor <= 0.0001 && !inVoice) ||
            _lastServer == null ||
            _lastChannel == null) {
          return const SizedBox.shrink();
        }
        final visibleHeight = (cardHeight * factor).clamp(0.0, cardHeight);
        final gapHeight = (OcFloats.gap * factor).clamp(0.0, OcFloats.gap);
        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SizedBox(height: gapHeight),
            SizedBox(
              key: ShellKeys.vcard,
              height: visibleHeight,
              child: ClipRRect(
                borderRadius: BorderRadius.circular(OcFloats.radius),
                child: OverflowBox(
                  alignment: Alignment.topCenter,
                  minHeight: cardHeight,
                  maxHeight: cardHeight,
                  child: OcFloat(
                    color: colors.sidebar,
                    child: VoiceConnectedPanel(
                      serverKey: inVoice ? voice.serverKey : _lastServer,
                      channelId: inVoice ? voice.channelId : _lastChannel,
                      inVoice: inVoice,
                    ),
                  ),
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}

/// The main column: main float with notch, and composer float underneath (§2).
class _MainColumn extends ConsumerWidget {
  const _MainColumn({
    required this.layout,
    required this.paneOpen,
    required this.onOpenPane,
    required this.onOpenSidebar,
  });

  final ShellLayout layout;
  final bool paneOpen;
  final VoidCallback onOpenPane;
  final VoidCallback? onOpenSidebar;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final homeShown = ref.watch(homeShownProvider);
    final currentPeer = ref.watch(currentPeerProvider);
    // Here, above the main float and the composer float: a provider that
    // first refreshes in one of them can't tell the other during the build.
    if (currentPeer != null) {
      ref.watch(contactNameProvider(currentPeer));
      ref.watch(sharedServersProvider(currentPeer));
    }

    final server = ref.watch(currentServerProvider);
    final channelId = ref.watch(currentChannelProvider);
    final channel = server == null
        ? null
        : ref.watch(
            serverProvider(
              server,
            ).select((state) => state.data?.channels[channelId]),
          );
    final open = server == null || channel == null
        ? null
        : (server: server, channel: channel.id);
    final noServers = ref.watch(
      serverListProvider.select((servers) => servers.isEmpty),
    );

    final bool isChat;
    if (homeShown) {
      isChat = currentPeer != null;
    } else {
      isChat = !noServers && open != null && channel!.kind.isTextLike;
    }

    Widget mainContent;
    Widget? composerContent;

    if (homeShown) {
      mainContent = HomeMain(
        onOpenSidebar: onOpenSidebar,
        includeComposer: false,
      );
      if (currentPeer != null) {
        composerContent = PrivateChatBottomBar(
          key: ValueKey(('private-composer', currentPeer)),
          peer: currentPeer,
        );
      }
    } else {
      mainContent = ChatArea(
        onOpenSidebar: onOpenSidebar,
        includeComposer: false,
      );
      if (isChat && open != null) {
        composerContent = Composer(
          key: ValueKey(('composer', open)),
          channel: open,
          controller: ref.read(chatControllerProvider),
        );
      }
    }

    final isChannelChat = !homeShown && isChat;
    final notch = ShellMetrics.maybeOf(context)!.notch;
    return AttachmentDropZone(
      channel: isChannelChat ? open : null,
      radius: OcFloats.radius,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            child: Stack(
              children: [
                Positioned.fill(
                  child: NotchedFloat(
                    key: ShellKeys.main,
                    notch: notch,
                    color: colors.chat,
                    child: mainContent,
                  ),
                ),
                if (!homeShown && !noServers)
                  Positioned(
                    top: 0,
                    right: 0,
                    child: PaneButton(
                      notch: notch,
                      open: paneOpen,
                      onOpen: onOpenPane,
                    ),
                  ),
              ],
            ),
          ),
          if (isChat && composerContent != null) ...[
            const SizedBox(height: OcFloats.gap),
            ConstrainedBox(
              constraints: const BoxConstraints(minHeight: OcFloats.bottom),
              child: OcFloat(
                key: ShellKeys.composer,
                color: colors.sidebar,
                child: composerContent,
              ),
            ),
          ],
        ],
      ),
    );
  }
}
