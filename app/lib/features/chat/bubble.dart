import 'package:flutter/material.dart';

import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/markdown.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

final _parsed = <String, List<MdBlock>>{};

/// Parsed markdown, cached: scrolling rebuilds rows often.
List<MdBlock> parsedContent(String content) {
  final cached = _parsed.remove(content);
  final blocks = cached ?? parseMarkdown(content);
  _parsed[content] = blocks;
  if (_parsed.length > 500) _parsed.remove(_parsed.keys.first);
  return blocks;
}

/// The message a reply points at, as the quote shows it.
@immutable
class ReplyPreview {
  const ReplyPreview({required this.author, required this.text});

  final String author;
  final String text;
}

/// Lets the text of one message be selected with the mouse (§4.5) without
/// adding a Tab stop or a menu of its own: the message's context menu
/// offers to copy the selection.
class SelectableMessageText extends StatefulWidget {
  const SelectableMessageText({
    super.key,
    required this.onSelectionChanged,
    required this.child,
  });

  final ValueChanged<String?> onSelectionChanged;
  final Widget child;

  @override
  State<SelectableMessageText> createState() => _SelectableMessageTextState();
}

class _SelectableMessageTextState extends State<SelectableMessageText> {
  final _focus = FocusNode(skipTraversal: true, debugLabel: 'message text');

  @override
  void dispose() {
    _focus.dispose();
    super.dispose();
  }

  /// The row's own context menu shows instead (see `MessageItem`). A null
  /// builder is not an option: SelectableRegion still calls it.
  static Widget _noMenu(BuildContext context, SelectableRegionState state) =>
      const SizedBox.shrink();

  @override
  Widget build(BuildContext context) => SelectionArea(
    focusNode: _focus,
    contextMenuBuilder: _noMenu,
    onSelectionChanged: (content) {
      // The space kept for the time is a placeholder character.
      final text = content?.plainText.replaceAll('\uFFFC', '').trim();
      widget.onSelectionChanged(text == null || text.isEmpty ? null : text);
    },
    child: widget.child,
  );
}

/// "👍 3": inverted when you reacted; a click toggles yours (§4.5).
class ReactionChip extends StatelessWidget {
  const ReactionChip({
    super.key,
    required this.reaction,
    this.names,
    this.onTap,
  });

  final Reaction reaction;
  final String? names;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final mine = reaction.me;
    final chip = Hoverable(
      onTap: onTap,
      semanticLabel:
          '${reaction.emoji} ${reaction.count}${mine ? ', including you' : ''}',
      selected: mine,
      focusRadius: BorderRadius.circular(12),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 24,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: mine ? colors.accent : colors.selected,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              reaction.emoji,
              style: const TextStyle(fontSize: 13, height: 1),
            ),
            const SizedBox(width: 5),
            Text(
              '${reaction.count}',
              style: OcText.small.copyWith(
                fontSize: 12.5,
                fontWeight: FontWeight.w500,
                height: 1,
                color: mine ? colors.onAccent : colors.text,
              ),
            ),
          ],
        ),
      ),
    );
    return names == null ? chip : Tooltip(message: names!, child: chip);
  }
}
