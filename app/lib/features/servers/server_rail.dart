import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/servers/rail_popover.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Controls the rail popover from items down the tree (§5.2).
abstract interface class RailPopoverController {
  void showPopover(Object id, BuildContext itemContext);
  void hidePopover(Object id);
  void dismissImmediately();
}

/// Scope exposing the rail popover controller to items down the tree.
class RailPopoverScope extends InheritedWidget {
  const RailPopoverScope({
    super.key,
    required this.controller,
    required super.child,
  });

  final RailPopoverController controller;

  static RailPopoverController? of(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<RailPopoverScope>()
      ?.controller;

  @override
  bool updateShouldNotify(RailPopoverScope oldWidget) =>
      controller != oldWidget.controller;
}

/// The server rail (§5.2): one icon per server, reordered by dragging,
/// with the rail popover showing on hover/focus.
class ServerRail extends ConsumerStatefulWidget {
  const ServerRail({super.key});

  @override
  ConsumerState<ServerRail> createState() => _ServerRailState();
}

class _ServerRailState extends ConsumerState<ServerRail>
    with TickerProviderStateMixin
    implements RailPopoverController {
  final _portalController = OverlayPortalController();
  late final AnimationController _scaleController;
  late final AnimationController _opacityController;
  late final AnimationController _yController;
  Timer? _leaveTimer;
  Object? _activeId;
  double _currentY = 0.0;
  double _targetY = 0.0;
  bool _visible = false;

  /// On its way out, the wait for the pointer over.
  bool _leaving = false;

  /// A rail item is being dragged: no popover until it lands.
  bool _reordering = false;

  @override
  void initState() {
    super.initState();
    _scaleController = AnimationController.unbounded(vsync: this, value: 0.96);
    _opacityController = AnimationController(vsync: this, value: 0.0);
    _yController = AnimationController.unbounded(vsync: this);
    _yController.addListener(() {
      setState(() {
        _currentY = _yController.value;
      });
    });
  }

  @override
  void dispose() {
    _leaveTimer?.cancel();
    _scaleController.dispose();
    _opacityController.dispose();
    _yController.dispose();
    super.dispose();
  }

  @override
  void showPopover(Object id, BuildContext itemContext) {
    final box = itemContext.findRenderObject() as RenderBox?;
    if (box == null || !box.attached) return;
    final center = box.localToGlobal(box.size.center(Offset.zero));
    _showAt(id, center.dy);
  }

  void _showAt(Object id, double y) {
    if (_reordering) return;
    _leaveTimer?.cancel();
    _leaveTimer = null;
    final motion = OcMotion.of(context);

    if (!_visible || _leaving || !_portalController.isShowing) {
      // From wherever a fade had got to, and with no slide, as the prototype.
      if (!_leaving) {
        _scaleController.value = 0.96;
        _opacityController.value = 0.0;
      }
      _leaving = false;
      _visible = true;
      _activeId = id;
      _yController.stop();
      _currentY = y;
      _targetY = y;
      if (!_portalController.isShowing) _portalController.show();
      if (motion.reduced) {
        _scaleController.value = 1.0;
      } else {
        _scaleController.animateWith(
          OcSpring.pop.simulation(
            _scaleController.value,
            1.0,
            _scaleController.velocity,
          ),
        );
      }
      _opacityController.animateTo(
        1.0,
        duration: motion.railPop,
        curve: OcMotion.curve,
      );
      setState(() {});
    } else {
      _activeId = id;
      if ((_targetY - y).abs() > 0.5) {
        final fromY = _currentY;
        _targetY = y;
        _yController.stop();
        if (motion.reduced) {
          _currentY = y;
        } else {
          _yController.value = fromY;
          _yController.animateWith(OcSpring.paneOut.simulation(fromY, y, 0.0));
        }
      }
      setState(() {});
    }
  }

  @override
  void hidePopover(Object id) {
    if (_activeId != id) return;
    _leaveTimer?.cancel();
    _leaveTimer = Timer(OcMotion.railPopWait, () {
      _leaveTimer = null;
      if (!mounted || !_visible) return;
      final fade = OcMotion.of(context).railPop;
      _leaving = true;
      _scaleController.animateTo(0.96, duration: fade, curve: OcMotion.curveIn);
      _opacityController
          .animateTo(0.0, duration: fade, curve: OcMotion.curveIn)
          .whenCompleteOrCancel(() {
            // Coming back turns the fade around and clears [_leaving].
            if (mounted && _leaving) dismissImmediately();
          });
    });
  }

  @override
  void dismissImmediately() {
    _leaveTimer?.cancel();
    _leaveTimer = null;
    _leaving = false;
    _scaleController.stop();
    _opacityController.stop();
    _yController.stop();
    _visible = false;
    _activeId = null;
    if (_portalController.isShowing) {
      _portalController.hide();
    }
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final servers = ref.watch(serverListProvider);

    ref.listen(serverListProvider, (prev, next) {
      if (_activeId is String && !next.any((s) => s.key == _activeId)) {
        dismissImmediately();
      }
    });

    return RailPopoverScope(
      controller: this,
      child: OverlayPortal(
        controller: _portalController,
        overlayChildBuilder: (context) {
          if (!_visible || _activeId == null) {
            return const SizedBox.shrink();
          }
          return Positioned(
            left: 78,
            top: _currentY,
            child: FractionalTranslation(
              translation: const Offset(0, -0.5),
              child: IgnorePointer(
                child: FadeTransition(
                  key: const Key('rail-popover-fade'),
                  opacity: _opacityController,
                  child: ScaleTransition(
                    key: const Key('rail-popover-scale'),
                    scale: _scaleController,
                    alignment: Alignment.centerLeft,
                    child: RailPopover(id: _activeId!),
                  ),
                ),
              ),
            ),
          );
        },
        child: ColoredBox(
          color: colors.sidebar,
          child: Column(
            children: [
              Padding(
                padding: const EdgeInsets.only(top: OcSpace.s8),
                child: Builder(
                  builder: (item) => Hoverable(
                    onTap: () =>
                        ref.read(navigationProvider.notifier).openHome(),
                    semanticLabel: 'Private messages',
                    onHover: (hovered) {
                      if (hovered) {
                        showPopover('private', item);
                      } else {
                        hidePopover('private');
                      }
                    },
                    onFocusChange: (focused) {
                      if (focused) {
                        showPopover('private', item);
                      } else {
                        hidePopover('private');
                      }
                    },
                    builder: (context, state) => RailItemView(
                      id: 'private',
                      name: 'Private',
                      glyph: OcIcons.chat,
                      selected: ref.watch(homeShownProvider),
                      hovered: state.active,
                      mentions: ref.watch(privateUnreadProvider),
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 6),
              Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 18,
                  vertical: 2,
                ),
                child: SizedBox(
                  width: double.infinity,
                  height: 1,
                  child: ColoredBox(color: colors.border),
                ),
              ),
              Expanded(
                child: ReorderableListView.builder(
                  // Each item ends in its gap of 6: 6 under the rule, 8 at the
                  // list's end.
                  padding: const EdgeInsets.only(top: 6, bottom: 2),
                  buildDefaultDragHandles: false,
                  itemCount: servers.length,
                  proxyDecorator: (child, index, animation) =>
                      Opacity(opacity: 0.9, child: child),
                  onReorderStart: (index) {
                    _reordering = true;
                    dismissImmediately();
                  },
                  onReorderEnd: (index) => _reordering = false,
                  onReorderItem: (from, to) => ref
                      .read(serverListProvider.notifier)
                      .move(servers[from].key, to),
                  itemBuilder: (context, index) => Padding(
                    key: ValueKey(servers[index].key),
                    padding: const EdgeInsets.only(bottom: 6),
                    child: ReorderableDragStartListener(
                      index: index,
                      child: _RailEntry(server: servers[index]),
                    ),
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(4, 0, 4, 4),
                child: Builder(
                  builder: (item) => Hoverable(
                    onTap: () => showAddServer(context),
                    semanticLabel: 'Add a server',
                    onHover: (hovered) {
                      if (hovered) {
                        showPopover('add', item);
                      } else {
                        hidePopover('add');
                      }
                    },
                    onFocusChange: (focused) {
                      if (focused) {
                        showPopover('add', item);
                      } else {
                        hidePopover('add');
                      }
                    },
                    builder: (context, state) => AnimatedContainer(
                      duration: motion.hover,
                      curve: OcMotion.curve,
                      height: 44,
                      decoration: BoxDecoration(
                        color: state.active ? colors.hover : Colors.transparent,
                        borderRadius: BorderRadius.circular(10),
                      ),
                      alignment: Alignment.center,
                      child: Icon(
                        OcIcons.add,
                        size: 20,
                        color: state.active
                            ? colors.text
                            : colors.textSecondary,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Where a server's connection stands, as the rail shows it (§5.2).
enum RailConnection { connected, connecting, failed }

String _stateLabel(ConnectionStatus status) => switch (status.phase) {
  ConnectionPhase.connected => 'Connected',
  ConnectionPhase.connecting => 'Connecting…',
  ConnectionPhase.reconnecting => 'Reconnecting…',
  ConnectionPhase.failed => 'Connection failed',
};

class _RailEntry extends ConsumerWidget {
  const _RailEntry({required this.server});

  final ServerSummary server;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final key = server.key;
    final selected =
        !ref.watch(homeShownProvider) &&
        ref.watch(currentServerProvider) == key;
    final status = ref.watch(serverProvider(key).select((s) => s.connection));
    final name =
        ref.watch(serverProvider(key).select((s) => s.data?.info.name)) ??
        server.name;
    final prefs = ref.watch(notificationPrefsProvider);
    final muted = prefs.serverMuted(key);
    final unread = ref.watch(
      activityProvider(key).select(
        (activity) => activity.channels.entries.any(
          (entry) => entry.value.unread && !prefs.muted(key, entry.key),
        ),
      ),
    );
    final mentions = ref.watch(activityProvider(key).select((a) => a.mentions));
    final scope = RailPopoverScope.of(context);

    return Hoverable(
      onTap: () => ref.read(navigationProvider.notifier).openServer(key),
      onSecondaryTap: (position) =>
          _showMenu(context, ref, position, name, muted),
      semanticLabel: '$name, ${_stateLabel(status)}',
      selected: selected,
      focusRadius: BorderRadius.circular(OcRadius.serverIconActive),
      onHover: (hovered) {
        if (hovered) {
          scope?.showPopover(key, context);
        } else {
          scope?.hidePopover(key);
        }
      },
      onFocusChange: (focused) {
        if (focused) {
          scope?.showPopover(key, context);
        } else {
          scope?.hidePopover(key);
        }
      },
      builder: (context, state) => RailItemView(
        id: key,
        name: name,
        icon: avatarImage(
          ref,
          key,
          ref.watch(serverProvider(key).select((s) => s.data?.info.iconHash)),
        ),
        selected: selected,
        hovered: state.active,
        unread: unread && !muted,
        mentions: muted ? 0 : mentions,
        connection: switch (status.phase) {
          ConnectionPhase.connected => RailConnection.connected,
          ConnectionPhase.connecting ||
          ConnectionPhase.reconnecting => RailConnection.connecting,
          ConnectionPhase.failed => RailConnection.failed,
        },
      ),
    );
  }

  /// Mark as read · Mute / Unmute notifications · Invite people ·
  /// Server settings (if permitted) · Copy address · — · Leave server.
  Future<void> _showMenu(
    BuildContext context,
    WidgetRef ref,
    Offset position,
    String name,
    bool muted,
  ) {
    final data = ref.read(serverProvider(server.key)).data;
    return showOcMenu(
      context: context,
      position: position,
      entries: [
        OcMenuItem(
          label: 'Mark as read',
          icon: OcIcons.markChatRead,
          onSelected: () =>
              ref.read(activityProvider(server.key).notifier).markAllRead(),
        ),
        OcMenuItem(
          label: muted ? 'Unmute notifications' : 'Mute notifications',
          icon: muted ? OcIcons.notifications : OcIcons.notificationsOff,
          onSelected: () => ref
              .read(notificationPrefsProvider.notifier)
              .toggleServer(server.key),
        ),
        if (data?.can(Permissions.createInvite) ?? false)
          OcMenuItem(
            label: 'Invite people',
            icon: OcIcons.personAdd,
            onSelected: () => showInvitePeople(context, serverKey: server.key),
          ),
        if (canOpenServerSettings(data))
          OcMenuItem(
            label: 'Server settings',
            icon: OcIcons.settings,
            onSelected: () =>
                showServerSettings(context, ref, serverKey: server.key),
          ),
        OcMenuItem(
          label: 'Copy address',
          icon: OcIcons.link,
          onSelected: () {
            Clipboard.setData(ClipboardData(text: server.key));
            showOcToast(context, 'Address copied');
          },
        ),
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Leave server',
          icon: OcIcons.logout,
          onSelected: () => confirmLeaveServer(context, ref, server.key, name),
        ),
      ],
    );
  }
}

/// Asks before forgetting a server (§4.1). Phase 1 has no "leave" request,
/// so the membership stays on the server and adding it again works.
Future<void> confirmLeaveServer(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  String name,
) async {
  final leave = await confirmAction(
    context,
    title: 'Leave $name?',
    message:
        '$name will be removed from this device. You stay a member, so you '
        'can add it again later.',
    action: 'Leave server',
  );
  if (leave) await ref.read(repositoryProvider).removeServer(serverKey);
}

/// One rail item (§5.2), separate from its data so every state can be
/// drawn on its own.
class RailItemView extends StatelessWidget {
  const RailItemView({
    super.key,
    required this.id,
    required this.name,
    this.glyph,
    this.selected = false,
    this.hovered = false,
    this.unread = false,
    this.mentions = 0,
    this.connection = RailConnection.connected,
    this.icon,
  });

  final Object id;
  final String name;

  /// Optional glyph drawn centered instead of initials.
  final IconData? glyph;

  /// The server's icon, when it has one.
  final ImageProvider? icon;
  final bool selected;
  final bool hovered;
  final bool unread;
  final int mentions;
  final RailConnection connection;

  static const double height = 52;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final pill = selected
        ? 34.0
        : hovered
        ? 18.0
        : unread
        ? 8.0
        : 0.0;
    return SizedBox(
      width: OcSize.railWidth,
      height: height,
      child: Stack(
        children: [
          Positioned(
            left: 0,
            top: 0,
            bottom: 0,
            child: Center(
              child: AnimatedContainer(
                duration: motion.morph,
                curve: OcMotion.curve,
                width: 4,
                height: pill,
                decoration: BoxDecoration(
                  color: colors.text,
                  borderRadius: const BorderRadius.horizontal(
                    right: Radius.circular(4),
                  ),
                ),
              ),
            ),
          ),
          Positioned(
            top: 3,
            left: 0,
            right: 0,
            child: Center(
              child: SizedBox.square(
                dimension: OcSize.serverIcon,
                child: Stack(
                  clipBehavior: Clip.none,
                  children: [
                    TweenAnimationBuilder<double>(
                      tween: Tween(
                        end: selected || hovered
                            ? OcRadius.serverIconActive
                            : OcRadius.serverIcon,
                      ),
                      duration: motion.morph,
                      curve: OcMotion.curve,
                      builder: (context, radius, _) {
                        final initials = Text(
                          initialsOf(name),
                          textScaler: TextScaler.noScaling,
                          style: OcText.bodyStrong.copyWith(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            height: 1,
                            color: selected ? colors.onAccent : colors.text,
                          ),
                        );
                        final centerContent = glyph != null
                            ? Icon(
                                glyph,
                                size: 20,
                                color: selected ? colors.onAccent : colors.text,
                              )
                            : initials;
                        final icon = this.icon;
                        return AnimatedContainer(
                          duration: motion.hover,
                          curve: OcMotion.curve,
                          width: OcSize.serverIcon,
                          height: OcSize.serverIcon,
                          alignment: Alignment.center,
                          clipBehavior: icon == null
                              ? Clip.none
                              : Clip.antiAlias,
                          decoration: BoxDecoration(
                            color: selected
                                ? colors.accent
                                : hovered
                                ? colors.selected
                                : colors.hover,
                            borderRadius: BorderRadius.circular(radius),
                          ),
                          child: icon == null
                              ? centerContent
                              : Image(
                                  image: icon,
                                  width: OcSize.serverIcon,
                                  height: OcSize.serverIcon,
                                  fit: BoxFit.cover,
                                  gaplessPlayback: true,
                                  errorBuilder: (context, error, stack) =>
                                      centerContent,
                                ),
                        );
                      },
                    ),
                    if (mentions > 0)
                      // The badge at -4, its ring outside that.
                      Positioned(
                        top: -6,
                        right: -6,
                        child: _Ringed(
                          ring: colors.sidebar,
                          radius: 11,
                          child: UnreadBadge(count: mentions),
                        ),
                      ),
                    if (connection != RailConnection.connected)
                      Positioned(
                        right: -3,
                        bottom: -3,
                        child: Container(
                          width: 18,
                          height: 18,
                          decoration: BoxDecoration(
                            color: colors.sidebar,
                            shape: BoxShape.circle,
                          ),
                          child: Icon(
                            connection == RailConnection.failed
                                ? OcIcons.error
                                : OcIcons.sync,
                            size: 12,
                            fill: 0,
                            color: colors.textSecondary,
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// A 2 px ring in the rail's color around a badge (§5.2).
class _Ringed extends StatelessWidget {
  const _Ringed({
    required this.ring,
    required this.radius,
    required this.child,
  });

  final Color ring;
  final double radius;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: ring,
        borderRadius: BorderRadius.circular(radius),
      ),
      child: child,
    );
  }
}
