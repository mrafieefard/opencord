import 'dart:async';

import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// How long a toast stays: 1.8 s (§4.12), or 5 s when it offers an action
/// such as Undo, so there is time to reach it.
const toastDuration = Duration(milliseconds: 1800);
const toastWithActionDuration = Duration(seconds: 5);

/// Distance from the top of the window, just under the header row.
const double toastTop = 72;

final _current = Expando<OverlayEntry>('toast');

/// Shows an inverted pill at the top of the window. A new toast replaces
/// the one on screen.
void showOcToast(
  BuildContext context,
  String message, {
  String? actionLabel,
  VoidCallback? onAction,
}) {
  final overlay = Overlay.of(context, rootOverlay: true);
  final themes = InheritedTheme.capture(from: context, to: overlay.context);
  _current[overlay]?.remove();
  late final OverlayEntry entry;
  entry = OverlayEntry(
    builder: (context) => themes.wrap(
      _Toast(
        message: message,
        actionLabel: actionLabel,
        onAction: onAction,
        duration: actionLabel == null ? toastDuration : toastWithActionDuration,
        onDone: () {
          if (_current[overlay] == entry) _current[overlay] = null;
          if (entry.mounted) entry.remove();
        },
      ),
    ),
  );
  _current[overlay] = entry;
  overlay.insert(entry);
}

class _Toast extends StatefulWidget {
  const _Toast({
    required this.message,
    required this.duration,
    required this.onDone,
    this.actionLabel,
    this.onAction,
  });

  final String message;
  final String? actionLabel;
  final VoidCallback? onAction;
  final Duration duration;
  final VoidCallback onDone;

  @override
  State<_Toast> createState() => _ToastState();
}

class _ToastState extends State<_Toast> with SingleTickerProviderStateMixin {
  late final AnimationController _fade = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 120),
  )..forward();
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer(widget.duration, _dismiss);
  }

  Future<void> _dismiss() async {
    _timer?.cancel();
    if (!mounted) return;
    await _fade.reverse();
    widget.onDone();
  }

  @override
  void dispose() {
    _timer?.cancel();
    _fade.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    if (OcMotion.of(context).hover == Duration.zero) _fade.value = 1;
    final action = widget.actionLabel;
    return Positioned(
      top: toastTop,
      left: 16,
      right: 16,
      child: Center(
        child: FadeTransition(
          opacity: _fade,
          child: Semantics(
            liveRegion: true,
            child: Container(
              constraints: const BoxConstraints(minHeight: 36),
              padding: EdgeInsets.only(
                left: 16,
                right: action == null ? 16 : 6,
              ),
              decoration: BoxDecoration(
                color: colors.accent,
                borderRadius: BorderRadius.circular(18),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Flexible(
                    child: Text(
                      widget.message,
                      style: OcText.body.copyWith(
                        color: colors.onAccent,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ),
                  if (action != null) ...[
                    const SizedBox(width: 8),
                    Hoverable(
                      onTap: () {
                        widget.onAction?.call();
                        _dismiss();
                      },
                      focusRadius: BorderRadius.circular(14),
                      builder: (context, state) => Container(
                        height: 28,
                        padding: const EdgeInsets.symmetric(horizontal: 10),
                        alignment: Alignment.center,
                        decoration: BoxDecoration(
                          borderRadius: BorderRadius.circular(14),
                          color: state.active
                              ? colors.onAccent.withValues(alpha: 0.14)
                              : null,
                        ),
                        child: Text(
                          action,
                          style: OcText.body.copyWith(
                            color: colors.onAccent,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
