import 'dart:async';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// Files dragged onto the chat show that attachments are coming (§16);
/// dropping them sends nothing.
class AttachmentDropZone extends StatefulWidget {
  const AttachmentDropZone({super.key, required this.child});

  /// How long the note stays after a drop, so it can be read.
  static const lingers = Duration(milliseconds: 2500);

  final Widget child;

  @override
  State<AttachmentDropZone> createState() => _AttachmentDropZoneState();
}

class _AttachmentDropZoneState extends State<AttachmentDropZone> {
  var _dragging = false;
  var _dropped = false;
  Timer? _hide;

  @override
  void dispose() {
    _hide?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return DropTarget(
      onDragEntered: (_) {
        _hide?.cancel();
        setState(() {
          _dragging = true;
          _dropped = false;
        });
      },
      onDragExited: (_) => setState(() => _dragging = false),
      onDragDone: (_) {
        setState(() {
          _dragging = false;
          _dropped = true;
        });
        _hide = Timer(AttachmentDropZone.lingers, () {
          if (mounted) setState(() => _dropped = false);
        });
      },
      child: Stack(
        fit: StackFit.passthrough,
        children: [
          widget.child,
          if (_dragging || _dropped)
            Positioned.fill(
              child: IgnorePointer(
                child: ColoredBox(
                  color: colors.scrim,
                  child: Center(
                    child: Container(
                      width: 320,
                      padding: const EdgeInsets.all(OcSpace.s24),
                      decoration: BoxDecoration(
                        color: colors.elevated,
                        borderRadius: BorderRadius.circular(OcRadius.dialog),
                        border: Border.all(color: colors.text, width: 1.5),
                      ),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(OcIcons.upload, size: 32, color: colors.text),
                          const SizedBox(height: OcSpace.s12),
                          Text(
                            'Attachments are coming later',
                            textAlign: TextAlign.center,
                            style: OcText.title.copyWith(color: colors.text),
                          ),
                          const SizedBox(height: OcSpace.s6),
                          Text(
                            'Opencord cannot send files yet.',
                            textAlign: TextAlign.center,
                            style: OcText.small.copyWith(
                              color: colors.textSecondary,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
