import 'package:flutter/foundation.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'overlays.dart';

/// What React Native draws around the area ("The plugin area channel",
/// `status`).
@immutable
class AreaStatus {
  const AreaStatus({
    required this.state,
    this.reason = '',
    this.detail = '',
    this.overlays = 0,
  });

  /// `loading`, `running`, `error`, `too_old` or `closed`.
  final String state;

  /// The `closed` op's reason.
  final String reason;

  /// The details an `error` op carries for those allowed to see them.
  final String detail;

  /// Open modals, sheets (named and in-tree) and dialogs.
  final int overlays;

  Map<String, Object?> toJson() => {
    'state': state,
    'reason': reason,
    'detail': detail,
    'overlays': overlays,
  };
}

AreaStatus statusOf(BlocksController controller) {
  final closed = controller.closed;
  if (closed != null) return AreaStatus(state: 'closed', reason: closed);
  if (controller.tooOld) return const AreaStatus(state: 'too_old');
  final error = controller.error;
  if (error != null) {
    return AreaStatus(state: 'error', detail: error.details ?? '');
  }
  final roots = controller.roots;
  if (roots == null) return const AreaStatus(state: 'loading');
  return AreaStatus(
    state: 'running',
    overlays: overlaysOf(controller).length,
  );
}
