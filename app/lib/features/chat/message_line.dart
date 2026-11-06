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

  static const double _avatarGap = 10;

  @override
  Widget build(BuildContext context) {
    final top = first ? 6.0 : 2.0;
    if (own) {
      return Padding(
        padding: EdgeInsets.only(top: top),
        child: Align(alignment: Alignment.centerRight, child: bubble),
      );
    }
    // The avatar hangs beside the bubble without adding to the row's
    // height, so moving it to a newer bubble never shifts the rows above.
    return Padding(
      padding: EdgeInsets.only(top: top),
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Padding(
            padding: const EdgeInsets.only(
              left: OcSize.messageAvatar + _avatarGap,
            ),
            child: Align(alignment: Alignment.centerLeft, child: bubble),
          ),
          if (avatar case final avatar?)
            Positioned(
              left: 0,
              bottom: 0,
              width: OcSize.messageAvatar,
              height: OcSize.messageAvatar,
              child: avatar,
            ),
        ],
      ),
    );
  }
}
