import 'dart:async';

import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Whether the chat shows the last cached state greyed out with the
/// banner (§4.13): while reconnecting, or after a failure other than a
/// changed identity, which has its own blocking dialog (§4.11).
bool serverUnreachable(ConnectionStatus status) => switch (status.phase) {
  ConnectionPhase.reconnecting => true,
  ConnectionPhase.failed => status.failure != FailureReason.fingerprintChanged,
  ConnectionPhase.connecting || ConnectionPhase.connected => false,
};

/// "Can't reach server · Retrying in 8 s" and Retry now (§4.13).
class ConnectionBanner extends ConsumerStatefulWidget {
  const ConnectionBanner({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ConnectionBanner> createState() => _ConnectionBannerState();
}

class _ConnectionBannerState extends ConsumerState<ConnectionBanner> {
  Timer? _tick;

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  /// Ticks once a second while there is a countdown to show.
  void _countDown(bool counting) {
    if (!counting) {
      _tick?.cancel();
      _tick = null;
    } else {
      _tick ??= Timer.periodic(
        const Duration(seconds: 1),
        (_) => setState(() {}),
      );
    }
  }

  String _text(ConnectionStatus status) {
    if (status.phase == ConnectionPhase.failed) {
      return "Can't connect · ${status.message ?? 'The server refused.'}";
    }
    final retryAt = status.retryAt;
    if (retryAt == null) return "Can't reach server";
    final left = retryAt.difference(clock.now()).inMilliseconds;
    if (left <= 0) return "Can't reach server · Retrying…";
    return "Can't reach server · Retrying in ${(left / 1000).ceil()} s";
  }

  @override
  Widget build(BuildContext context) {
    final status = ref.watch(
      serverProvider(widget.serverKey).select((state) => state.connection),
    );
    final shown = serverUnreachable(status);
    _countDown(shown && status.retryAt != null);
    if (!shown) return const SizedBox.shrink();
    final colors = context.oc;
    final retry =
        status.phase == ConnectionPhase.reconnecting ||
        status.failure == FailureReason.incompatible;
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s8,
      ),
      decoration: BoxDecoration(
        color: colors.hover,
        border: Border(bottom: BorderSide(color: colors.border)),
      ),
      child: Semantics(
        liveRegion: true,
        child: Row(
          children: [
            Icon(OcIcons.cloudOff, size: OcSize.iconRow, color: colors.text),
            const SizedBox(width: OcSpace.s10),
            Expanded(
              child: Text(
                _text(status),
                style: OcText.body.copyWith(color: colors.text),
              ),
            ),
            if (retry) ...[
              const SizedBox(width: OcSpace.s12),
              OcButton(
                label: 'Retry now',
                dense: true,
                onPressed: () =>
                    ref.read(repositoryProvider).retryNow(widget.serverKey),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
