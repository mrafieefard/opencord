import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/emoji/emoji.dart';
import 'package:opencord/ui/emoji/emoji_data.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// Opens the emoji picker next to [anchor] and returns the chosen emoji.
Future<String?> showEmojiPicker(
  BuildContext context, {
  required Rect anchor,
  PopoverSide side = PopoverSide.above,
  bool alignEnd = true,
}) => showPopover<String>(
  context: context,
  anchor: anchor,
  side: side,
  alignEnd: alignEnd,
  padding: const EdgeInsets.all(OcSpace.s8),
  builder: (context) =>
      EmojiPicker(onPicked: (emoji) => Navigator.pop(context, emoji)),
);

/// A section of the picker's grid.
@immutable
class EmojiSection {
  const EmojiSection(this.label, this.icon, this.emoji);

  final String label;
  final IconData icon;
  final List<Emoji> emoji;
}

/// Moves a keyboard selection through sections laid out [columns] wide,
/// each section starting a new row. Returns the new (section, index).
(int, int) moveInGrid(
  List<int> lengths,
  (int, int) at,
  LogicalKeyboardKey key, {
  int columns = EmojiPicker.columns,
}) {
  final (section, index) = at;
  final length = lengths[section];
  int? previousSection() {
    for (var s = section - 1; s >= 0; s--) {
      if (lengths[s] > 0) return s;
    }
    return null;
  }

  int? nextSection() {
    for (var s = section + 1; s < lengths.length; s++) {
      if (lengths[s] > 0) return s;
    }
    return null;
  }

  final column = index % columns;
  if (key == LogicalKeyboardKey.arrowRight) {
    if (index + 1 < length) return (section, index + 1);
    final next = nextSection();
    return next == null ? at : (next, 0);
  }
  if (key == LogicalKeyboardKey.arrowLeft) {
    if (index > 0) return (section, index - 1);
    final previous = previousSection();
    return previous == null ? at : (previous, lengths[previous] - 1);
  }
  if (key == LogicalKeyboardKey.arrowDown) {
    final row = index ~/ columns;
    if ((row + 1) * columns < length) {
      return (
        section,
        (row + 1) * columns + column < length
            ? (row + 1) * columns + column
            : length - 1,
      );
    }
    final next = nextSection();
    return next == null
        ? at
        : (next, column < lengths[next] ? column : lengths[next] - 1);
  }
  if (key == LogicalKeyboardKey.arrowUp) {
    if (index >= columns) return (section, index - columns);
    final previous = previousSection();
    if (previous == null) return at;
    final lastRow = (lengths[previous] - 1) ~/ columns;
    final target = lastRow * columns + column;
    return (
      previous,
      target < lengths[previous] ? target : lengths[previous] - 1,
    );
  }
  return at;
}

/// The emoji picker (§4.5, §4.6): search, category tabs, frequently used
/// first, and the hovered emoji's name and shortcode underneath. Arrow
/// keys and Enter work from the search field.
class EmojiPicker extends ConsumerStatefulWidget {
  const EmojiPicker({super.key, required this.onPicked});

  final ValueChanged<String> onPicked;

  static const int columns = 8;
  static const double width = 344;
  static const double gridHeight = 300;
  static const double cell = 40;
  static const double header = 28;

  @override
  ConsumerState<EmojiPicker> createState() => _EmojiPickerState();
}

final _arrows = {
  LogicalKeyboardKey.arrowLeft,
  LogicalKeyboardKey.arrowRight,
  LogicalKeyboardKey.arrowUp,
  LogicalKeyboardKey.arrowDown,
};

class _EmojiPickerState extends ConsumerState<EmojiPicker> {
  final _search = TextEditingController();
  final _scroll = ScrollController();
  String _query = '';
  (int, int) _selected = (0, 0);
  Emoji? _hovered;

  static final Map<EmojiCategory, List<Emoji>> _byCategory = {
    for (final category in EmojiCategory.values)
      category: [
        for (final emoji in emojiData)
          if (emoji.category == category) emoji,
      ],
  };

  @override
  void dispose() {
    _search.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<EmojiSection> _sections() {
    if (_query.isNotEmpty) {
      return [
        EmojiSection(
          'Results',
          OcIcons.search,
          searchEmoji(_query, limit: 160),
        ),
      ];
    }
    final frequent = [
      for (final char in frequentEmoji(
        ref.watch(emojiUsageProvider),
        count: 16,
      ))
        ?emojiForChar(char),
    ];
    return [
      EmojiSection('Frequently used', OcIcons.history, frequent),
      for (final category in EmojiCategory.values)
        EmojiSection(category.label, category.icon, _byCategory[category]!),
    ];
  }

  /// Where [section] starts in the grid: every section is a header plus
  /// whole rows of fixed height.
  double _offsetOf(List<EmojiSection> sections, int section) {
    var offset = 0.0;
    for (var s = 0; s < section; s++) {
      final rows = (sections[s].emoji.length / EmojiPicker.columns).ceil();
      offset += EmojiPicker.header + rows * EmojiPicker.cell;
    }
    return offset;
  }

  void _reveal(List<EmojiSection> sections, (int, int) at) {
    if (!_scroll.hasClients) return;
    final (section, index) = at;
    final top =
        _offsetOf(sections, section) +
        EmojiPicker.header +
        (index ~/ EmojiPicker.columns) * EmojiPicker.cell;
    final position = _scroll.position;
    if (top < position.pixels) {
      position.jumpTo(top - EmojiPicker.header);
    } else if (top + EmojiPicker.cell >
        position.pixels + position.viewportDimension) {
      position.jumpTo(top + EmojiPicker.cell - position.viewportDimension);
    }
  }

  void _pick(Emoji emoji) {
    ref.read(emojiUsageProvider.notifier).use(emoji.char);
    widget.onPicked(emoji.char);
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final sections = _sections();
    final lengths = [for (final section in sections) section.emoji.length];
    if (lengths.every((length) => length == 0)) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      final (section, index) = _selected;
      if (index < lengths[section]) _pick(sections[section].emoji[index]);
      return KeyEventResult.handled;
    }
    // Arrows move through the grid even while typing, like Discord's
    // picker; everything else goes to the search field.
    if (!_arrows.contains(key)) return KeyEventResult.ignored;
    final moved = moveInGrid(lengths, _selected, key);
    setState(() => _selected = moved);
    _reveal(sections, moved);
    return KeyEventResult.handled;
  }

  /// The first section whose header has scrolled past the top.
  int _currentSection(List<EmojiSection> sections) {
    if (!_scroll.hasClients) return 0;
    final pixels = _scroll.position.pixels + 1;
    var current = 0;
    for (var s = 0; s < sections.length; s++) {
      if (_offsetOf(sections, s) <= pixels) current = s;
    }
    return current;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final sections = _sections();
    final (selectedSection, selectedIndex) = _selected;
    final selected =
        selectedSection < sections.length &&
            selectedIndex < sections[selectedSection].emoji.length
        ? sections[selectedSection].emoji[selectedIndex]
        : null;
    final shown = _hovered ?? selected;
    return SizedBox(
      width: EmojiPicker.width,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Focus(
            onKeyEvent: _onKey,
            child: OcTextField(
              controller: _search,
              autofocus: true,
              hint: 'Search emoji',
              prefixIcon: OcIcons.search,
              semanticLabel: 'Search emoji',
              onChanged: (value) => setState(() {
                _query = value.trim();
                _selected = (0, 0);
                if (_scroll.hasClients) _scroll.jumpTo(0);
              }),
              onSubmitted: (_) {},
            ),
          ),
          const SizedBox(height: OcSpace.s6),
          if (_query.isEmpty)
            ListenableBuilder(
              listenable: _scroll,
              builder: (context, _) {
                final current = _currentSection(sections);
                return Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    for (final (index, section) in sections.indexed)
                      OcIconButton(
                        icon: section.icon,
                        tooltip: section.label,
                        size: OcIconButtonSize.compact,
                        active: index == current,
                        onPressed: () => _scroll.jumpTo(
                          _offsetOf(
                            sections,
                            index,
                          ).clamp(0, _scroll.position.maxScrollExtent),
                        ),
                      ),
                  ],
                );
              },
            ),
          SizedBox(
            height: EmojiPicker.gridHeight,
            child: sections.every((section) => section.emoji.isEmpty)
                ? Center(
                    child: Text(
                      'No emoji match "$_query"',
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  )
                : CustomScrollView(
                    controller: _scroll,
                    slivers: [
                      for (final (s, section) in sections.indexed) ...[
                        SliverToBoxAdapter(
                          child: SizedBox(
                            height: EmojiPicker.header,
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: Text(
                                section.label.toUpperCase(),
                                style: OcText.label.copyWith(
                                  color: colors.textMuted,
                                ),
                              ),
                            ),
                          ),
                        ),
                        SliverGrid.builder(
                          gridDelegate:
                              const SliverGridDelegateWithFixedCrossAxisCount(
                                crossAxisCount: EmojiPicker.columns,
                                mainAxisExtent: EmojiPicker.cell,
                              ),
                          itemCount: section.emoji.length,
                          itemBuilder: (context, index) => _EmojiCell(
                            emoji: section.emoji[index],
                            selected: _selected == (s, index),
                            onHover: (hovered) => setState(
                              () => _hovered = hovered
                                  ? section.emoji[index]
                                  : (_hovered == section.emoji[index]
                                        ? null
                                        : _hovered),
                            ),
                            onTap: () => _pick(section.emoji[index]),
                          ),
                        ),
                      ],
                    ],
                  ),
          ),
          Container(
            height: 44,
            margin: const EdgeInsets.only(top: OcSpace.s6),
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            decoration: BoxDecoration(
              border: Border(top: BorderSide(color: colors.border)),
            ),
            child: shown == null
                ? null
                : Row(
                    children: [
                      Text(shown.char, style: const TextStyle(fontSize: 26)),
                      const SizedBox(width: OcSpace.s10),
                      Expanded(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              ':${shown.shortcode}:',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.small.copyWith(
                                fontWeight: FontWeight.w600,
                                color: colors.text,
                              ),
                            ),
                            Text(
                              shown.name,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.meta.copyWith(
                                color: colors.textMuted,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

class _EmojiCell extends StatelessWidget {
  const _EmojiCell({
    required this.emoji,
    required this.selected,
    required this.onHover,
    required this.onTap,
  });

  final Emoji emoji;
  final bool selected;
  final ValueChanged<bool> onHover;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    // Cells are no Tab stops: arrow keys move through the grid from the
    // search field.
    return MouseRegion(
      onEnter: (_) => onHover(true),
      onExit: (_) => onHover(false),
      child: ExcludeFocus(
        child: Hoverable(
          onTap: onTap,
          semanticLabel: emoji.name,
          focusRadius: BorderRadius.circular(OcRadius.menu),
          builder: (context, state) => Container(
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: selected || state.active ? colors.selected : null,
              borderRadius: BorderRadius.circular(OcRadius.menu),
            ),
            child: Text(
              emoji.char,
              style: const TextStyle(fontSize: 24, height: 1.1),
            ),
          ),
        ),
      ),
    );
  }
}
