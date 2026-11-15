import 'package:flutter/material.dart';

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/thinking_dots.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// A command's public result (Phase 3 plan §17.3): one line, "**Kai** used
/// `/play` · Added *Song* to the queue". While a plugin thinks the reply is
/// "**Tickets** is thinking…", and "Tickets didn't answer" if it never came.
class CommandResultLine extends StatelessWidget {
  const CommandResultLine({
    super.key,
    required this.message,
    required this.author,
    required this.plugin,
    required this.links,
  });

  final Message message;

  /// Who ran it.
  final String author;

  /// Who answers: the plugin, or whoever ran an Opencord command.
  final String plugin;
  final MarkdownContext links;

  List<InlineSpan> _lead(OcColors colors) => [
    TextSpan(
      text: author,
      style: OcText.bodyStrong.copyWith(color: colors.text),
    ),
    TextSpan(
      text: ' used ',
      style: OcText.body.copyWith(color: colors.textMuted),
    ),
    TextSpan(
      text: '/${message.command}',
      style: OcText.mono.copyWith(
        color: colors.text,
        backgroundColor: colors.hover,
      ),
    ),
    TextSpan(
      text: ' · ',
      style: OcText.body.copyWith(color: colors.textMuted),
    ),
  ];

  Widget _thinking(OcColors colors) => Text.rich(
    TextSpan(
      children: [
        ..._lead(colors),
        TextSpan(
          text: plugin,
          style: OcText.bodyStrong.copyWith(color: colors.text),
        ),
        TextSpan(
          text: ' is thinking…',
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
        WidgetSpan(
          alignment: PlaceholderAlignment.middle,
          child: ThinkingDots(color: colors.textMuted),
        ),
      ],
    ),
  );

  Widget _noAnswer(OcColors colors) => Text.rich(
    TextSpan(
      children: [
        ..._lead(colors),
        WidgetSpan(
          alignment: PlaceholderAlignment.middle,
          child: Padding(
            padding: const EdgeInsets.only(right: OcSpace.s4),
            child: Icon(OcIcons.schedule, size: 14, color: colors.textMuted),
          ),
        ),
        TextSpan(
          text: message.content,
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s4,
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 3, right: OcSpace.s8),
            child: Icon(
              OcIcons.chevronRight,
              size: OcSize.iconInline,
              color: colors.textMuted,
            ),
          ),
          Expanded(
            child: switch (message.commandState) {
              CommandRunState.thinking => _thinking(colors),
              CommandRunState.noAnswer => _noAnswer(colors),
              CommandRunState.answered => MarkdownView(
                blocks: parsedContent(message.content),
                links: links,
                color: colors.textSecondary,
                lead: _lead(colors),
              ),
            },
          ),
        ],
      ),
    );
  }
}

/// A reply only this device sees (§17.3): muted, "Only you can see this ·
/// Dismiss".
class EphemeralLine extends StatelessWidget {
  const EphemeralLine({
    super.key,
    required this.note,
    required this.links,
    required this.onDismiss,
  });

  final EphemeralNote note;
  final MarkdownContext links;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      margin: const EdgeInsets.symmetric(
        horizontal: OcSpace.s12,
        vertical: OcSpace.s4,
      ),
      padding: const EdgeInsets.all(OcSpace.s10),
      decoration: BoxDecoration(
        border: Border.all(color: colors.border),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          switch (note.state) {
            CommandRunState.thinking => Row(
              children: [
                Flexible(
                  child: Text(
                    note.content,
                    style: OcText.body.copyWith(color: colors.textMuted),
                  ),
                ),
                ThinkingDots(color: colors.textMuted),
              ],
            ),
            CommandRunState.noAnswer => Row(
              children: [
                Icon(OcIcons.schedule, size: 14, color: colors.textMuted),
                const SizedBox(width: OcSpace.s4),
                Flexible(
                  child: Text(
                    note.content,
                    style: OcText.body.copyWith(color: colors.textMuted),
                  ),
                ),
              ],
            ),
            CommandRunState.answered => MarkdownView(
              blocks: parsedContent(note.content),
              links: links,
              color: note.error ? colors.text : colors.textSecondary,
            ),
          },
          const SizedBox(height: OcSpace.s4),
          Row(
            children: [
              Icon(OcIcons.visibility, size: 13, color: colors.textMuted),
              const SizedBox(width: OcSpace.s4),
              Text(
                'Only you can see this · ',
                style: OcText.meta.copyWith(color: colors.textMuted),
              ),
              Hoverable(
                onTap: onDismiss,
                semanticLabel: 'Dismiss',
                focusRadius: BorderRadius.circular(4),
                builder: (context, state) => Text(
                  'Dismiss',
                  style: OcText.meta.copyWith(
                    color: colors.textSecondary,
                    decoration: state.hovered ? TextDecoration.underline : null,
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
