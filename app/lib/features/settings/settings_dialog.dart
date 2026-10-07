import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// One page of a settings dialog.
@immutable
class SettingsPage {
  const SettingsPage({
    required this.id,
    required this.label,
    required this.icon,
    required this.group,
    required this.builder,
  });

  final String id;
  final String label;
  final IconData icon;

  /// The heading it sits under in the navigation.
  final String group;
  final WidgetBuilder builder;
}

/// Opens a settings dialog on [initialPage] (the first page when null).
Future<void> showSettingsDialog(
  BuildContext context, {
  required String title,
  required List<SettingsPage> pages,
  String? initialPage,
}) => showOcDialog<void>(
  context: context,
  builder: (context) =>
      SettingsDialog(title: title, pages: pages, initialPage: initialPage),
);

/// The settings shell (§8): Telegram-like pages with Discord's navigation.
/// Up to 980 × 700, a 240 px sidebar of grouped pages, and the page under a
/// 60 px title bar with a close button.
class SettingsDialog extends StatefulWidget {
  const SettingsDialog({
    super.key,
    required this.title,
    required this.pages,
    this.initialPage,
  });

  final String title;
  final List<SettingsPage> pages;
  final String? initialPage;

  static const Size maxSize = Size(980, 700);
  static const double navWidth = 240;

  @override
  State<SettingsDialog> createState() => _SettingsDialogState();
}

class _SettingsDialogState extends State<SettingsDialog> {
  late String _page =
      widget.pages
          .where((page) => page.id == widget.initialPage)
          .firstOrNull
          ?.id ??
      widget.pages.first.id;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final screen = MediaQuery.sizeOf(context);
    final page = widget.pages.firstWhere(
      (candidate) => candidate.id == _page,
      orElse: () => widget.pages.first,
    );
    final groups = <String, List<SettingsPage>>{};
    for (final candidate in widget.pages) {
      groups.putIfAbsent(candidate.group, () => []).add(candidate);
    }
    return Center(
      child: SizedBox(
        width: math.min(SettingsDialog.maxSize.width, screen.width - 48),
        height: math.min(SettingsDialog.maxSize.height, screen.height - 48),
        child: Material(
          type: MaterialType.transparency,
          child: Container(
            clipBehavior: Clip.antiAlias,
            decoration: BoxDecoration(
              color: colors.elevated,
              borderRadius: BorderRadius.circular(OcRadius.dialog),
              border: Border.all(color: colors.border),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Container(
                  width: SettingsDialog.navWidth,
                  color: colors.sidebar,
                  child: ListView(
                    padding: const EdgeInsets.all(OcSpace.s12),
                    children: [
                      Padding(
                        padding: const EdgeInsets.fromLTRB(
                          OcSpace.s8,
                          OcSpace.s8,
                          OcSpace.s8,
                          OcSpace.s4,
                        ),
                        child: Semantics(
                          header: true,
                          child: Text(
                            widget.title,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: OcText.title.copyWith(color: colors.text),
                          ),
                        ),
                      ),
                      for (final MapEntry(key: group, value: items)
                          in groups.entries) ...[
                        SectionLabel(
                          group,
                          padding: const EdgeInsets.fromLTRB(
                            OcSpace.s8,
                            OcSpace.s16,
                            OcSpace.s8,
                            OcSpace.s6,
                          ),
                        ),
                        for (final item in items)
                          _NavItem(
                            page: item,
                            selected: item.id == page.id,
                            onTap: () => setState(() => _page = item.id),
                          ),
                      ],
                    ],
                  ),
                ),
                VerticalDivider(width: 1, color: colors.border),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Container(
                        height: OcSize.header,
                        padding: const EdgeInsets.only(
                          left: OcSpace.s24,
                          right: OcSpace.s12,
                        ),
                        decoration: BoxDecoration(
                          border: Border(
                            bottom: BorderSide(color: colors.border),
                          ),
                        ),
                        child: Row(
                          children: [
                            Expanded(
                              child: Semantics(
                                header: true,
                                child: Text(
                                  page.label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: OcText.title.copyWith(
                                    color: colors.text,
                                  ),
                                ),
                              ),
                            ),
                            OcIconButton(
                              icon: OcIcons.close,
                              tooltip: 'Close settings',
                              onPressed: () => Navigator.pop(context),
                            ),
                          ],
                        ),
                      ),
                      Expanded(
                        child: SingleChildScrollView(
                          key: PageStorageKey(page.id),
                          padding: const EdgeInsets.all(OcSpace.s24),
                          child: KeyedSubtree(
                            key: ValueKey(page.id),
                            child: page.builder(context),
                          ),
                        ),
                      ),
                    ],
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

class _NavItem extends StatelessWidget {
  const _NavItem({
    required this.page,
    required this.selected,
    required this.onTap,
  });

  final SettingsPage page;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: page.label,
      selected: selected,
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        decoration: BoxDecoration(
          color: selected
              ? colors.selected
              : state.active
              ? colors.hover
              : null,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Row(
          children: [
            Icon(
              page.icon,
              size: OcSize.iconRow,
              color: selected ? colors.text : colors.textSecondary,
            ),
            const SizedBox(width: OcSpace.s10),
            Expanded(
              child: Text(
                page.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.body.copyWith(
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                  color: selected ? colors.text : colors.textSecondary,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
