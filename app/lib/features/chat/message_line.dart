import 'package:flutter/widgets.dart';

import 'package:opencord/ui/theme/oc_metrics.dart';

/// Lays out one message (§4.5): incoming bubbles on the left with the
/// author's avatar beside the last bubble of a group, your own on the right
/// without one. Groups sit 8 px apart, bubbles within a group 2 px.
class MessageLine extends StatelessWidget {
  const MessageLine({
    super.key,
    required this.own,
    required this.first,
    required this.bubble,
    this.avatar,
  });

  final bool own;
  final bool first;
  final Widget bubble;

  /// Shown on the last bubble of an incoming group; the slot stays empty on
  /// the others so the bubbles line up.
  final Widget? avatar;

  /// Space kept free on the far side, so a long bubble never touches it.
  static const double gutter = 48;
  static const double _avatarGap = OcSpace.s8;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(top: first ? OcSpace.s8 : OcSpace.s2),
      child: own
          ? Padding(
              padding: const EdgeInsets.only(left: gutter),
              child: Align(alignment: Alignment.centerRight, child: bubble),
            )
          : Padding(
              padding: const EdgeInsets.only(right: gutter),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  SizedBox(width: OcSize.messageAvatar, child: avatar),
                  const SizedBox(width: _avatarGap),
                  Flexible(child: bubble),
                ],
              ),
            ),
    );
  }
}
