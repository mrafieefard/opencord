import 'dart:io';

import 'package:flutter/material.dart';

import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

IconData channelIcon(ChannelKind kind) => switch (kind) {
  ChannelKind.text => OcIcons.tag,
  ChannelKind.announcement => OcIcons.campaign,
  ChannelKind.voice => OcIcons.volumeUp,
  ChannelKind.category => OcIcons.expandMore,
  ChannelKind.plugin => OcIcons.extension,
};

/// The rounded square in front of a channel row (§4.2), inverted when the
/// row is selected, with a small lock for read-only or private channels.
class ChannelGlyph extends StatelessWidget {
  const ChannelGlyph({
    super.key,
    required this.kind,
    this.size = OcSize.channelGlyph,
    this.selected = false,
    this.locked = false,
    this.ringColor,
    this.iconFile,
    this.unavailable = false,
  });

  final ChannelKind kind;
  final double size;
  final bool selected;
  final bool locked;

  /// The color behind the glyph, used to cut the lock badge out of it.
  final Color? ringColor;

  /// Optional local file path for custom plugin channel icon.
  final String? iconFile;

  /// Greyed out when unavailable.
  final bool unavailable;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final iconColor = unavailable
        ? colors.textMuted
        : selected
        ? colors.onAccent
        : colors.textSecondary;
    Widget iconWidget;
    if (iconFile != null && iconFile!.isNotEmpty) {
      iconWidget = Image.file(
        File(iconFile!),
        width: size * 0.5,
        height: size * 0.5,
        color: unavailable ? colors.textMuted : null,
        errorBuilder: (_, _, _) =>
            Icon(channelIcon(kind), size: size * 0.5, color: iconColor),
      );
    } else {
      iconWidget = Icon(channelIcon(kind), size: size * 0.5, color: iconColor);
    }
    final glyph = Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: selected ? colors.accent : colors.selected,
        borderRadius: BorderRadius.circular(size * OcRadius.channelGlyph / 40),
      ),
      child: iconWidget,
    );
    if (!locked) return glyph;
    return SizedBox.square(
      dimension: size,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          glyph,
          Positioned(
            right: -3,
            bottom: -3,
            child: Container(
              width: 16,
              height: 16,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: ringColor ?? colors.sidebar,
              ),
              alignment: Alignment.center,
              child: Icon(
                OcIcons.lock,
                size: 10,
                fill: 1,
                color: colors.textSecondary,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
