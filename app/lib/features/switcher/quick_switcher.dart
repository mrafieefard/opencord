import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/members/member_profile.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/switcher/switcher_results.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/key_hint.dart';

/// Opens the quick switcher (§4.9) and goes where the user picks.
Future<void> showQuickSwitcher(BuildContext context, WidgetRef ref) async {
  final colors = context.oc;
  final picked = await showGeneralDialog<SwitcherItem>(
    context: context,
    barrierDismissible: true,
    barrierLabel: 'Close',
    barrierColor: colors.scrim,
    transitionDuration: OcMotion.of(context).dialog,
    pageBuilder: (context, animation, secondaryAnimation) =>
        const QuickSwitcher(),
    transitionBuilder: (context, animation, secondaryAnimation, child) {
      final curved = CurvedAnimation(parent: animation, curve: Curves.easeOut);
      return FadeTransition(
        opacity: curved,
        child: ScaleTransition(
          scale: Tween<double>(begin: 0.98, end: 1).animate(curved),
          child: child,
        ),
      );
    },
  );
  if (picked == null || !context.mounted) return;
  final navigation = ref.read(navigationProvider.notifier);
  switch (picked) {
    case SwitcherChannel(:final serverKey, :final channel):
      navigation.openChannel(serverKey, channel.id);
    case SwitcherServer(:final serverKey):
      navigation.openServer(serverKey);
    case SwitcherMember(:final serverKey, :final member):
      await showMemberProfile(context, serverKey: serverKey, userId: member.id);
  }
}

/// Everything the switcher can jump to: channels the user can see on every
/// server, the servers, and the members of the open server.
List<SwitcherItem> switcherItems(WidgetRef ref) {
  final current = ref.watch(currentServerProvider);
  final items = <SwitcherItem>[];
  final servers = ref.watch(serverListProvider);
  for (final server in servers) {
    final data = ref.watch(serverProvider(server.key).select((s) => s.data));
    final name = data?.info.name ?? server.name;
    items.add(SwitcherServer(serverKey: server.key, name: name));
    if (data == null) continue;
    for (final channel in data.channels.values) {
      if (channel.isCategory) continue;
      if (!data.permissionsIn(channel.id).has(Permissions.viewChannel)) {
        continue;
      }
      items.add(
        SwitcherChannel(
          serverKey: server.key,
          serverName: name,
          channel: channel,
        ),
      );
    }
    if (server.key == current) {
      for (final member in data.members.values) {
        items.add(SwitcherMember(serverKey: server.key, member: member));
      }
    }
  }
  return items;
}

/// The switcher panel: 600 px, 110 px from the top, a large input and the
/// results, with key hints underneath.
class QuickSwitcher extends ConsumerStatefulWidget {
  const QuickSwitcher({super.key});

  static const double width = 600;
  static const double top = 110;
  static const double rowHeight = 48;

  @override
  ConsumerState<QuickSwitcher> createState() => _QuickSwitcherState();
}

class _QuickSwitcherState extends ConsumerState<QuickSwitcher> {
  final _query = TextEditingController();
  final _scroll = ScrollController();
  int _selected = 0;

  @override
  void dispose() {
    _query.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<SwitcherItem> _results() => rankSwitcher(
    switcherItems(ref),
    _query.text,
    recent: ref.watch(recentChannelsProvider),
  );

  void _move(int step, int count) {
    if (count == 0) return;
    setState(() => _selected = (_selected + step) % count);
    if (!_scroll.hasClients) return;
    final top = _selected * QuickSwitcher.rowHeight;
    final position = _scroll.position;
    if (top < position.pixels) {
      position.jumpTo(top);
    } else if (top + QuickSwitcher.rowHeight >
        position.pixels + position.viewportDimension) {
      position.jumpTo(
        top + QuickSwitcher.rowHeight - position.viewportDimension,
      );
    }
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final results = _results();
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.arrowDown) {
      _move(1, results.length);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.arrowUp) {
      _move(-1, results.length);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      if (results.isNotEmpty) {
        Navigator.pop(context, results[_selected.clamp(0, results.length - 1)]);
      }
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final results = _results();
    final selected = results.isEmpty
        ? -1
        : _selected.clamp(0, results.length - 1);
    final screen = MediaQuery.sizeOf(context);
    final platform = Theme.of(context).platform;
    return Align(
      alignment: Alignment.topCenter,
      child: Padding(
        padding: EdgeInsets.only(
          top: math.min(QuickSwitcher.top, screen.height * 0.12),
        ),
        child: SizedBox(
          width: math.min(QuickSwitcher.width, screen.width - 2 * OcSpace.s16),
          child: Material(
            type: MaterialType.transparency,
            child: Container(
              decoration: BoxDecoration(
                color: colors.elevated,
                borderRadius: BorderRadius.circular(OcRadius.dialog),
                border: Border.all(color: colors.border),
                boxShadow: OcShadows.menu(colors),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Focus(
                    canRequestFocus: false,
                    skipTraversal: true,
                    onKeyEvent: _onKey,
                    child: TextField(
                      controller: _query,
                      autofocus: true,
                      style: OcText.title.copyWith(
                        fontWeight: FontWeight.w400,
                        color: colors.text,
                      ),
                      cursorColor: colors.text,
                      onChanged: (_) => setState(() => _selected = 0),
                      decoration: InputDecoration(
                        hintText: 'Jump to a channel, server or member',
                        hintStyle: OcText.title.copyWith(
                          fontWeight: FontWeight.w400,
                          color: colors.textMuted,
                        ),
                        filled: false,
                        border: InputBorder.none,
                        enabledBorder: InputBorder.none,
                        focusedBorder: InputBorder.none,
                        contentPadding: const EdgeInsets.fromLTRB(
                          OcSpace.s4,
                          OcSpace.s16,
                          OcSpace.s16,
                          OcSpace.s16,
                        ),
                        prefixIcon: Icon(
                          OcIcons.search,
                          size: OcSize.iconButton,
                          color: colors.textMuted,
                        ),
                        prefixIconConstraints: const BoxConstraints(
                          minWidth: 48,
                        ),
                      ),
                    ),
                  ),
                  Divider(height: 1, color: colors.border),
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxHeight: math.min(
                        QuickSwitcher.rowHeight * 8 + OcSpace.s12,
                        screen.height * 0.5,
                      ),
                    ),
                    child: results.isEmpty
                        ? Padding(
                            padding: const EdgeInsets.all(OcSpace.s24),
                            child: Text(
                              'Nothing matches "${_query.text.trim()}".',
                              textAlign: TextAlign.center,
                              style: OcText.body.copyWith(
                                color: colors.textMuted,
                              ),
                            ),
                          )
                        : ListView.builder(
                            controller: _scroll,
                            shrinkWrap: true,
                            padding: const EdgeInsets.all(OcSpace.s6),
                            itemExtent: QuickSwitcher.rowHeight,
                            itemCount: results.length,
                            itemBuilder: (context, index) => _ResultRow(
                              item: results[index],
                              selected: index == selected,
                              onHover: () => setState(() => _selected = index),
                              onTap: () =>
                                  Navigator.pop(context, results[index]),
                            ),
                          ),
                  ),
                  Divider(height: 1, color: colors.border),
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s16,
                      vertical: OcSpace.s10,
                    ),
                    child: Wrap(
                      spacing: OcSpace.s16,
                      runSpacing: OcSpace.s6,
                      children: [
                        _Hint(
                          keys: const [
                            SingleActivator(LogicalKeyboardKey.arrowUp),
                            SingleActivator(LogicalKeyboardKey.arrowDown),
                          ],
                          label: 'Navigate',
                          platform: platform,
                        ),
                        _Hint(
                          keys: const [
                            SingleActivator(LogicalKeyboardKey.enter),
                          ],
                          label: 'Open',
                          platform: platform,
                        ),
                        _Hint(
                          keys: const [
                            SingleActivator(LogicalKeyboardKey.escape),
                          ],
                          label: 'Close',
                          platform: platform,
                        ),
                        Text(
                          '#  @  *  narrow to channels, members, servers',
                          style: OcText.meta.copyWith(color: colors.textMuted),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _Hint extends StatelessWidget {
  const _Hint({
    required this.keys,
    required this.label,
    required this.platform,
  });

  final List<SingleActivator> keys;
  final String label;
  final TargetPlatform platform;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      for (final key in keys) ...[KeyHint(key), const SizedBox(width: 3)],
      const SizedBox(width: 3),
      Text(label, style: OcText.meta.copyWith(color: context.oc.textMuted)),
    ],
  );
}

class _ResultRow extends StatelessWidget {
  const _ResultRow({
    required this.item,
    required this.selected,
    required this.onHover,
    required this.onTap,
  });

  final SwitcherItem item;
  final bool selected;
  final VoidCallback onHover;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final (Widget leading, String subtitle) = switch (item) {
      SwitcherChannel(:final channel, :final serverName) => (
        ChannelGlyph(kind: channel.kind, size: 30),
        serverName,
      ),
      SwitcherServer(:final serverKey, :final name) => (
        OcAvatar(id: serverKey, name: name, size: 30, borderRadius: 9),
        'Server',
      ),
      SwitcherMember(:final member) => (
        OcAvatar(id: '${member.id}', name: member.displayName, size: 30),
        'Member',
      ),
    };
    final name = switch (item) {
      SwitcherChannel(:final channel) when channel.kind.isTextLike =>
        '#${channel.name}',
      _ => item.name,
    };
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      onEnter: (_) => onHover(),
      child: GestureDetector(
        onTap: onTap,
        child: Semantics(
          button: true,
          selected: selected,
          label: '$name, $subtitle',
          excludeSemantics: true,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s10),
            decoration: BoxDecoration(
              color: selected ? colors.selected : null,
              borderRadius: BorderRadius.circular(OcRadius.row),
            ),
            child: Row(
              children: [
                leading,
                const SizedBox(width: OcSpace.s12),
                Expanded(
                  child: Text(
                    name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.body.copyWith(
                      fontWeight: FontWeight.w500,
                      color: colors.text,
                    ),
                  ),
                ),
                const SizedBox(width: OcSpace.s12),
                Text(
                  subtitle,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.small.copyWith(color: colors.textMuted),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
