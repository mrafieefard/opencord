import 'package:flutter/material.dart';

import 'package:opencord/features/chat/suggestions.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/popup_route.dart';

/// The autocomplete above the composer (§4.6). Arrow keys and Enter or Tab
/// work from the input, so the rows never take the keyboard focus.
class SuggestionList extends StatelessWidget {
  const SuggestionList({
    super.key,
    required this.width,
    required this.suggestions,
    required this.selected,
    required this.onHover,
    required this.onPick,
  });

  final double width;
  final List<Suggestion> suggestions;
  final int selected;
  final ValueChanged<int> onHover;
  final ValueChanged<Suggestion> onPick;

  String get _title => switch (suggestions.firstOrNull) {
    MemberSuggestion() => 'Members',
    ChannelSuggestion() => 'Channels',
    EmojiSuggestion() => 'Emoji',
    null => '',
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Material(
      type: MaterialType.transparency,
      child: Container(
        width: width,
        padding: const EdgeInsets.all(OcSpace.s4),
        decoration: popupDecoration(colors),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s8,
                OcSpace.s4,
                OcSpace.s8,
                OcSpace.s4,
              ),
              child: Text(
                _title.toUpperCase(),
                style: OcText.label.copyWith(color: colors.textMuted),
              ),
            ),
            for (final (index, suggestion) in suggestions.indexed)
              MouseRegion(
                cursor: SystemMouseCursors.click,
                onEnter: (_) => onHover(index),
                child: GestureDetector(
                  onTap: () => onPick(suggestion),
                  child: Semantics(
                    button: true,
                    selected: index == selected,
                    child: Container(
                      height: 36,
                      padding: const EdgeInsets.symmetric(
                        horizontal: OcSpace.s8,
                      ),
                      decoration: BoxDecoration(
                        color: index == selected ? colors.selected : null,
                        borderRadius: BorderRadius.circular(OcRadius.menu - 2),
                      ),
                      child: _SuggestionRow(suggestion: suggestion),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _SuggestionRow extends StatelessWidget {
  const _SuggestionRow({required this.suggestion});

  final Suggestion suggestion;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final name = OcText.body.copyWith(color: colors.text);
    final muted = OcText.small.copyWith(color: colors.textMuted);
    return Row(
      children: switch (suggestion) {
        MemberSuggestion(:final member, :final role) => [
          OcAvatar(id: '${member.id}', name: member.displayName, size: 24),
          const SizedBox(width: OcSpace.s10),
          Flexible(
            child: Text(
              member.displayName,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: name,
            ),
          ),
          if (role != null) ...[
            const SizedBox(width: OcSpace.s8),
            Text(role, style: muted),
          ],
        ],
        ChannelSuggestion(:final channel) => [
          Icon(OcIcons.tag, size: OcSize.iconRow, color: colors.textSecondary),
          const SizedBox(width: OcSpace.s10),
          Flexible(
            child: Text(
              channel.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: name,
            ),
          ),
        ],
        EmojiSuggestion(:final emoji) => [
          SizedBox(
            width: 24,
            child: Text(emoji.char, style: const TextStyle(fontSize: 18)),
          ),
          const SizedBox(width: OcSpace.s10),
          Flexible(
            child: Text(
              ':${emoji.shortcode}:',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: name,
            ),
          ),
        ],
      },
    );
  }
}
