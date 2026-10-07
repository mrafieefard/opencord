import 'package:flutter/material.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';

/// A centered pill: date separators and system notices (§4.5).
class CenterPill extends StatelessWidget {
  const CenterPill({super.key, required this.text, this.icon});

  final String text;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Center(
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        decoration: BoxDecoration(
          color: colors.selected,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (icon != null) ...[
              Icon(icon, size: 14, color: colors.textSecondary),
              const SizedBox(width: 6),
            ],
            Flexible(
              child: Text(
                text,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: OcText.small.copyWith(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: colors.textSecondary,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// What a system notice says.
String systemText(SystemEvent event, String actor) => switch (event) {
  SystemEvent.memberJoined => '$actor joined the server',
  SystemEvent.channelCreated => '$actor created the channel',
  SystemEvent.messagePinned => '$actor pinned a message',
};

IconData systemIcon(SystemEvent event) => switch (event) {
  SystemEvent.memberJoined => OcIcons.personAdd,
  SystemEvent.channelCreated => OcIcons.tag,
  SystemEvent.messagePinned => OcIcons.pushPin,
};

/// "Unread messages" across the full width (§4.5).
class UnreadLine extends StatelessWidget {
  const UnreadLine({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Semantics(
      header: true,
      child: Row(
        children: [
          Expanded(child: Divider(height: 1, color: colors.textMuted)),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s10),
            child: Text(
              'Unread messages',
              style: OcText.small.copyWith(
                fontWeight: FontWeight.w600,
                color: colors.textSecondary,
              ),
            ),
          ),
          Expanded(child: Divider(height: 1, color: colors.textMuted)),
        ],
      ),
    );
  }
}

/// The top of a channel's history (§4.5).
class StartOfChannel extends StatelessWidget {
  const StartOfChannel({super.key, required this.kind, required this.name});

  final ChannelKind kind;
  final String name;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final title = kind.isTextLike ? '#$name' : name;
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s8,
        OcSpace.s32,
        OcSpace.s8,
        OcSpace.s16,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ChannelGlyph(kind: kind, size: 64),
          const SizedBox(height: OcSpace.s12),
          Text(
            'Welcome to $title',
            style: OcText.title.copyWith(fontSize: 22, color: colors.text),
          ),
          const SizedBox(height: OcSpace.s4),
          Text(
            'This is the start of the $title channel.',
            style: OcText.body.copyWith(color: colors.textSecondary),
          ),
        ],
      ),
    );
  }
}
