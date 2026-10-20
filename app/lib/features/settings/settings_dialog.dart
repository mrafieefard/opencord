import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

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

/// Changes made on settings pages that wait for the save bar, as in
/// Discord: nothing applies until Save changes, and Discard drops them.
@immutable
class PendingChanges {
  const PendingChanges({
    required this.dirty,
    required this.save,
    required this.discard,
  });

  final bool Function() dirty;

  /// Applies them; a [RepoException] says why it couldn't.
  final Future<void> Function() save;
  final VoidCallback discard;
}

/// The changes waiting for a settings dialog's save bar. Whoever holds some
/// registers them, and calls [touch] whenever they may have changed.
class SettingsChanges extends ChangeNotifier {
  final _pending = <Object, PendingChanges>{};
  var _disposed = false;

  /// From a page's `didChangeDependencies`, or `dispose` for [unregister]:
  /// the bar hears of it a moment later, as the widget tree can't be
  /// rebuilt while it is being built or taken down.
  void register(Object owner, PendingChanges changes) {
    _pending[owner] = changes;
    _notifyLater();
  }

  void unregister(Object owner) {
    if (_pending.remove(owner) != null) _notifyLater();
  }

  void _notifyLater() => scheduleMicrotask(() {
    if (!_disposed) notifyListeners();
  });

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }

  void touch() => notifyListeners();

  bool get dirty => _pending.values.any((changes) => changes.dirty());

  /// Saves each that holds changes, stopping at the first that fails.
  Future<void> save() async {
    for (final changes in List.of(_pending.values)) {
      if (changes.dirty()) await changes.save();
    }
    notifyListeners();
  }

  void discard() {
    for (final changes in List.of(_pending.values)) {
      changes.discard();
    }
    notifyListeners();
  }
}

/// Lets settings pages say they hold changes not saved yet, so closing the
/// dialog asks first (§16 "never lose input"), and hand changes to the save
/// bar ([changes]).
class SettingsScope extends InheritedWidget {
  const SettingsScope({
    super.key,
    required this.unsaved,
    required this.changes,
    required super.child,
  });

  final Map<Object, bool Function()> unsaved;
  final SettingsChanges changes;

  /// The save bar's changes for the dialog around [context], if any.
  static SettingsChanges? changesOf(BuildContext context) =>
      context.getInheritedWidgetOfExactType<SettingsScope>()?.changes;

  /// The checks of the dialog around [context], if any. Pages add
  /// themselves in `didChangeDependencies` and remove themselves from the
  /// same map in `dispose`, when the dialog can no longer be looked up.
  static Map<Object, bool Function()>? of(BuildContext context) =>
      context.getInheritedWidgetOfExactType<SettingsScope>()?.unsaved;

  @override
  bool updateShouldNotify(SettingsScope oldWidget) => false;
}

/// Asks before changes not saved yet in [unsaved] or [changes] are dropped
/// (§16 "never lose input"): true when there are none, or the person lets
/// them go, which discards [changes].
Future<bool> _mayDiscard(
  BuildContext context,
  Map<Object, bool Function()>? unsaved,
  SettingsChanges? changes,
) async {
  final pageUnsaved = unsaved?.values.any((check) => check()) ?? false;
  final barUnsaved = changes?.dirty ?? false;
  if (!pageUnsaved && !barUnsaved) return true;
  final discard = await confirmAction(
    context,
    title: 'Discard your changes?',
    message: 'Some changes are not saved yet.',
    action: 'Discard',
  );
  if (discard) changes?.discard();
  return discard;
}

/// For pages that swap what they edit (a role, a channel): asks first when
/// the settings around [context] hold unsaved changes.
Future<bool> confirmLeavingUnsaved(BuildContext context) => _mayDiscard(
  context,
  SettingsScope.of(context),
  SettingsScope.changesOf(context),
);

/// Opens a settings dialog on [initialPage] (the first page when null).
Future<void> showSettingsDialog(
  BuildContext context, {
  required String title,
  required List<SettingsPage> pages,
  String? initialPage,
  Widget? binding,
}) => showOcDialog<void>(
  context: context,
  builder: (context) => SettingsDialog(
    title: title,
    pages: pages,
    initialPage: initialPage,
    binding: binding,
  ),
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
    this.binding,
  });

  final String title;
  final List<SettingsPage> pages;
  final String? initialPage;

  /// Built once for the whole dialog, out of sight: hands changes kept
  /// apart from the pages (client settings' drafts) to the save bar.
  final Widget? binding;

  static const Size maxSize = Size(980, 700);
  static const double navWidth = 240;

  @override
  State<SettingsDialog> createState() => _SettingsDialogState();
}

class _SettingsDialogState extends State<SettingsDialog> {
  final _unsaved = <Object, bool Function()>{};
  final _changes = SettingsChanges();

  @override
  void dispose() {
    _changes.dispose();
    super.dispose();
  }

  Future<void> _close() async {
    if (!await _mayDiscard(context, _unsaved, _changes) || !mounted) return;
    Navigator.pop(context);
  }

  Future<void> _open(String page) async {
    if (page == _page) return;
    if (!await _mayDiscard(context, _unsaved, _changes) || !mounted) return;
    setState(() => _page = page);
  }

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
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _close();
      },
      child: SettingsScope(
        unsaved: _unsaved,
        changes: _changes,
        child: Center(
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
                                style: OcText.title.copyWith(
                                  color: colors.text,
                                ),
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
                                onTap: () => _open(item.id),
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
                                  onPressed: _close,
                                ),
                              ],
                            ),
                          ),
                          Expanded(
                            child: Stack(
                              children: [
                                Positioned.fill(
                                  child: SingleChildScrollView(
                                    key: PageStorageKey(page.id),
                                    // Room for the save bar over the end.
                                    padding: const EdgeInsets.fromLTRB(
                                      OcSpace.s24,
                                      OcSpace.s24,
                                      OcSpace.s24,
                                      OcSpace.s24 + SaveBar.height,
                                    ),
                                    child: KeyedSubtree(
                                      key: ValueKey(page.id),
                                      child: page.builder(context),
                                    ),
                                  ),
                                ),
                                Positioned(
                                  left: OcSpace.s16,
                                  right: OcSpace.s16,
                                  bottom: OcSpace.s16,
                                  child: SaveBar(changes: _changes),
                                ),
                                ?widget.binding,
                              ],
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

/// Discord's bar for changes not saved yet: it rises from the bottom while
/// there are some, with Discard and Save changes.
class SaveBar extends StatefulWidget {
  const SaveBar({super.key, required this.changes});

  final SettingsChanges changes;

  static const double height = 56;

  @override
  State<SaveBar> createState() => _SaveBarState();
}

class _SaveBarState extends State<SaveBar> {
  var _saving = false;

  /// Shown, or still sliding away; offstage once gone.
  var _onstage = false;

  Future<void> _save() async {
    setState(() => _saving = true);
    try {
      await widget.changes.save();
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    return ListenableBuilder(
      listenable: widget.changes,
      builder: (context, _) {
        final shown = widget.changes.dirty || _saving;
        if (shown) _onstage = true;
        return Offstage(
          offstage: !_onstage,
          child: IgnorePointer(
            ignoring: !shown,
            child: AnimatedSlide(
              offset: shown ? Offset.zero : const Offset(0, 1.6),
              duration: motion.saveBar,
              curve: Curves.easeOutCubic,
              onEnd: () {
                if (!shown && mounted) setState(() => _onstage = false);
              },
              child: AnimatedOpacity(
                opacity: shown ? 1 : 0,
                duration: motion.saveBar,
                child: Container(
                  height: SaveBar.height,
                  padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
                  decoration: BoxDecoration(
                    color: colors.surface,
                    borderRadius: BorderRadius.circular(OcRadius.bubble),
                    border: Border.all(color: colors.border),
                    boxShadow: OcShadows.menu(colors),
                  ),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Careful — you have unsaved changes!',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.body.copyWith(color: colors.text),
                        ),
                      ),
                      OcButton.ghost(
                        label: 'Discard',
                        onPressed: _saving ? null : widget.changes.discard,
                      ),
                      const SizedBox(width: OcSpace.s8),
                      OcButton.primary(
                        label: 'Save changes',
                        busy: _saving,
                        onPressed: _save,
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
