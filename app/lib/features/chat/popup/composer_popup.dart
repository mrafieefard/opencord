import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:opencord/core/bidi.dart';
import 'package:opencord/features/chat/popup/popup_rail.dart';
import 'package:opencord/features/chat/popup/popup_rows.dart';
import 'package:opencord/features/chat/popup/source_icon.dart';
import 'package:opencord/features/chat/suggestions.dart';
import 'package:opencord/features/music/music_artwork.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/pop_in.dart';
import 'package:opencord/ui/widgets/popup_route.dart';

/// The popup's sizes, from the approved prototype.
abstract final class PopupMetrics {
  static const double radius = 14;
  static const double row = 36;

  /// Autocomplete rows with artwork.
  static const double tallRow = 40;
  static const double header = 30;
  static const double cap = 340;
  static const double gap = 8;
  static const double _bottomPad = 4;
  static const double _emptyHeight = 42;
}

/// The list above the composer: commands by source with their rail, or
/// one titled list of mentions, channels, emoji or an option's choices.
/// The arrow keys stay in the input, so the rows never take the focus.
class ComposerPopup extends StatefulWidget {
  const ComposerPopup({
    super.key,
    required this.serverKey,
    required this.width,
    required this.maxHeight,
    this.groups = const [],
    this.rows = const [],
    this.title = '',
    this.rowHeight = PopupMetrics.row,
    required this.selected,
    required this.onHover,
    required this.onPick,
    this.emptyText,
  });

  final String serverKey;
  final double width;
  final double maxHeight;

  /// Commands by source.
  final List<PopupGroup> groups;

  /// Everything else, under [title].
  final List<Suggestion> rows;
  final String title;
  final double rowHeight;

  /// Counted across every group.
  final int selected;
  final ValueChanged<int> onHover;
  final ValueChanged<Suggestion> onPick;

  /// Shown when there's nothing to list.
  final String? emptyText;

  @override
  State<ComposerPopup> createState() => ComposerPopupState();
}

class ComposerPopupState extends State<ComposerPopup> {
  final _scroll = ScrollController();
  var _railIndex = 0;

  /// Where a rail click went: the list may not be able to bring that group
  /// to the top, and the highlight stays on it until the list moves.
  double? _railOffset;

  bool get _hasRail => widget.groups.length >= 2;

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_onScroll);
  }

  @override
  void didUpdateWidget(ComposerPopup oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_listKey(oldWidget) == _listKey(widget)) return;
    _railIndex = 0;
    _railOffset = null;
    if (_scroll.hasClients) _scroll.jumpTo(0);
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  /// Changes when filtering or a command list update changes what's shown.
  static String _listKey(ComposerPopup popup) {
    final first = popup.groups.firstOrNull?.rows.firstOrNull;
    return '${popup.groups.length}/${popup.rows.length}/'
        '${popup.groups.fold(0, (sum, group) => sum + group.rows.length)}/'
        '${first?.spec.source}/${first?.spec.name}';
  }

  void _onScroll() {
    if (!_hasRail) return;
    final offset = _scroll.offset;
    if (_railOffset case final at? when (offset - at).abs() < 1) return;
    _railOffset = null;
    final at = groupAtOffset(
      widget.groups,
      offset,
      row: PopupMetrics.row,
      header: PopupMetrics.header,
    );
    if (at != _railIndex) setState(() => _railIndex = at);
  }

  void _goToGroup(int index) {
    final position = _scroll.position;
    final target = math.min(
      offsetOfRow(
        widget.groups,
        widget.groups[index].firstIndex,
        row: PopupMetrics.row,
        header: PopupMetrics.header,
      ),
      position.maxScrollExtent,
    );
    _railOffset = target;
    _scroll.jumpTo(target);
    setState(() => _railIndex = index);
    widget.onHover(widget.groups[index].firstIndex);
  }

  /// Brings the group holding row [index] to the top.
  void scrollToGroupOf(int index) {
    if (!_scroll.hasClients) return;
    _scroll.jumpTo(
      math.min(
        offsetOfRow(
          widget.groups,
          index,
          row: PopupMetrics.row,
          header: PopupMetrics.header,
        ),
        _scroll.position.maxScrollExtent,
      ),
    );
  }

  /// Keeps row [index] in view, under its sticky header.
  void scrollToRow(int index) {
    if (!_scroll.hasClients) return;
    final position = _scroll.position;
    final top = widget.groups.isNotEmpty
        ? offsetOfRow(
            widget.groups,
            index,
            row: PopupMetrics.row,
            header: PopupMetrics.header,
          )
        : index * widget.rowHeight;
    final bottom =
        top + PopupMetrics.header + widget.rowHeight + PopupMetrics._bottomPad;
    if (top < position.pixels) {
      _scroll.jumpTo(top);
    } else if (bottom > position.pixels + position.viewportDimension) {
      _scroll.jumpTo(
        (bottom - position.viewportDimension).clamp(
          position.minScrollExtent,
          position.maxScrollExtent,
        ),
      );
    }
  }

  double get _contentHeight {
    if (widget.groups.isEmpty && widget.rows.isEmpty) {
      return PopupMetrics._emptyHeight;
    }
    final sections = widget.groups.isNotEmpty
        ? widget.groups.fold(
            0.0,
            (sum, group) =>
                sum +
                PopupMetrics.header +
                group.rows.length * PopupMetrics.row,
          )
        : PopupMetrics.header + widget.rows.length * widget.rowHeight;
    // The border is drawn inside the popup's own height.
    return sections + PopupMetrics._bottomPad + 2;
  }

  Widget _row(Suggestion suggestion, int index) => ComposerPopupRow(
    serverKey: widget.serverKey,
    suggestion: suggestion,
    selected: index == widget.selected,
    onHover: () {
      if (index != widget.selected) widget.onHover(index);
    },
    onTap: () => widget.onPick(suggestion),
  );

  Widget _section({
    required Widget header,
    required int count,
    required double rowHeight,
    required Widget Function(int index) row,
  }) => SliverMainAxisGroup(
    slivers: [
      SliverPersistentHeader(pinned: true, delegate: _Header(header)),
      SliverPadding(
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s4),
        sliver: SliverFixedExtentList(
          itemExtent: rowHeight,
          delegate: SliverChildBuilderDelegate(
            (context, index) => row(index),
            childCount: count,
          ),
        ),
      ),
    ],
  );

  Widget _list(BuildContext context) {
    final colors = context.oc;
    if (widget.groups.isEmpty && widget.rows.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s10,
        ),
        child: Text(
          widget.emptyText ?? '',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: OcText.small.copyWith(color: colors.textMuted),
        ),
      );
    }
    return CustomScrollView(
      controller: _scroll,
      slivers: [
        if (widget.groups.isNotEmpty)
          for (final group in widget.groups)
            _section(
              header: _SourceHeader(
                serverKey: widget.serverKey,
                source: group.source,
              ),
              count: group.rows.length,
              rowHeight: PopupMetrics.row,
              row: (index) => _row(group.rows[index], group.firstIndex + index),
            )
        else
          _section(
            header: _TitleHeader(widget.title),
            count: widget.rows.length,
            rowHeight: widget.rowHeight,
            row: (index) => _row(widget.rows[index], index),
          ),
        const SliverToBoxAdapter(
          child: SizedBox(height: PopupMetrics._bottomPad),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return PopIn(
      child: Material(
        type: MaterialType.transparency,
        child: Container(
          width: widget.width,
          height: math.max(0, math.min(_contentHeight, widget.maxHeight)),
          clipBehavior: Clip.antiAlias,
          decoration: popupDecoration(
            colors,
          ).copyWith(borderRadius: BorderRadius.circular(PopupMetrics.radius)),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_hasRail)
                PopupRail(
                  serverKey: widget.serverKey,
                  groups: widget.groups,
                  selected: _railIndex,
                  onPick: _goToGroup,
                ),
              Expanded(child: _list(context)),
            ],
          ),
        ),
      ),
    );
  }
}

class _Header extends SliverPersistentHeaderDelegate {
  const _Header(this.child);

  final Widget child;

  @override
  double get minExtent => PopupMetrics.header;

  @override
  double get maxExtent => PopupMetrics.header;

  @override
  Widget build(
    BuildContext context,
    double shrinkOffset,
    bool overlapsContent,
  ) => SizedBox.expand(
    child: ColoredBox(
      color: context.oc.elevated,
      child: Align(alignment: Alignment.centerLeft, child: child),
    ),
  );

  @override
  bool shouldRebuild(_Header old) => true;
}

class _TitleHeader extends StatelessWidget {
  const _TitleHeader(this.title);

  final String title;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
    child: Align(
      alignment: Alignment.centerLeft,
      child: Text(
        title.toUpperCase(),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: OcText.label.copyWith(color: context.oc.textMuted),
      ),
    ),
  );
}

class _SourceHeader extends StatelessWidget {
  const _SourceHeader({required this.serverKey, required this.source});

  final String serverKey;
  final PopupSource source;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
    child: Row(
      children: [
        CommandSourceIcon(
          serverKey: serverKey,
          source: source,
          size: 16,
          radius: 5,
        ),
        const SizedBox(width: OcSpace.s6),
        Expanded(
          child: Text(
            source.name.toUpperCase(),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: OcText.label.copyWith(color: context.oc.textMuted),
          ),
        ),
      ],
    ),
  );
}

/// One row of the popup.
class ComposerPopupRow extends StatelessWidget {
  const ComposerPopupRow({
    super.key,
    required this.serverKey,
    required this.suggestion,
    required this.selected,
    required this.onHover,
    required this.onTap,
  });

  final String serverKey;
  final Suggestion suggestion;
  final bool selected;
  final VoidCallback onHover;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      // Only a moving pointer: rows scrolling under a still one must not
      // take the selection from the keyboard.
      onHover: (_) => onHover(),
      child: GestureDetector(
        onTap: onTap,
        child: Semantics(
          button: true,
          selected: selected,
          child: AnimatedContainer(
            duration: OcMotion.of(context).hover,
            curve: OcMotion.curve,
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            decoration: BoxDecoration(
              color: selected ? colors.selected : Colors.transparent,
              borderRadius: BorderRadius.circular(PopupMetrics.radius - 4),
            ),
            child: _RowContent(serverKey: serverKey, suggestion: suggestion),
          ),
        ),
      ),
    );
  }
}

class _RowContent extends StatelessWidget {
  const _RowContent({required this.serverKey, required this.suggestion});

  final String serverKey;
  final Suggestion suggestion;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final name = OcText.body.copyWith(color: colors.text);
    final muted = OcText.small.copyWith(color: colors.textMuted);
    Widget oneLine(String text, TextStyle style) => Text(
      text,
      maxLines: 1,
      softWrap: false,
      overflow: TextOverflow.ellipsis,
      style: style,
    );
    return Row(
      children: switch (suggestion) {
        CommandSuggestion(:final spec, :final unavailable) => () {
          final dim = unavailable == null ? 1.0 : 0.5;
          final description = unavailable ?? spec.description;
          return [
            Opacity(
              opacity: dim,
              child: Text(
                '/${spec.name}',
                maxLines: 1,
                style: OcText.bodyStrong.copyWith(color: colors.text),
              ),
            ),
            if (spec.options.isNotEmpty) ...[
              const SizedBox(width: OcSpace.s10),
              Flexible(
                child: Opacity(
                  opacity: dim,
                  child: oneLine(
                    spec.options.map((option) => option.name).join('  '),
                    muted,
                  ),
                ),
              ),
            ],
            const SizedBox(width: OcSpace.s10),
            Expanded(
              flex: 3,
              child: Text(
                description,
                maxLines: 1,
                softWrap: false,
                overflow: TextOverflow.ellipsis,
                textDirection: textDirectionOf(description),
                textAlign: TextAlign.start,
                style: muted,
              ),
            ),
          ];
        }(),
        MemberSuggestion(:final member, :final role) => [
          OcAvatar(id: '${member.id}', name: member.displayName, size: 24),
          const SizedBox(width: OcSpace.s10),
          Flexible(child: oneLine(member.displayName, name)),
          if (role != null) ...[
            const SizedBox(width: OcSpace.s8),
            Text(role, style: muted),
          ],
        ],
        ChannelSuggestion(:final channel) => [
          Icon(OcIcons.tag, size: OcSize.iconRow, color: colors.textSecondary),
          const SizedBox(width: OcSpace.s10),
          Flexible(child: oneLine(channel.name, name)),
        ],
        ChoiceSuggestion(:final choice) => [
          if (choice.artworkId.isNotEmpty || choice.name.contains(' · ')) ...[
            MusicArtwork(
              serverKey: serverKey,
              artworkId: choice.artworkId,
              size: 28,
              radius: 4,
            ),
            const SizedBox(width: OcSpace.s8),
          ],
          Expanded(child: oneLine(choice.name, name)),
        ],
        EmojiSuggestion(:final emoji) => [
          SizedBox(
            width: 24,
            child: Text(emoji.char, style: const TextStyle(fontSize: 18)),
          ),
          const SizedBox(width: OcSpace.s10),
          Flexible(child: oneLine(':${emoji.shortcode}:', name)),
        ],
      },
    );
  }
}
