import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The debug-only menu of plan §11 (Ctrl+Shift+F12): edge states on the
/// mock, and the widget gallery.
Future<void> showDebugMenu(BuildContext context, WidgetRef ref) {
  final repository = ref.read(repositoryProvider);
  final mock = repository is MockRepository ? repository : null;
  final server = ref.read(currentServerProvider);
  final size = MediaQuery.sizeOf(context);
  void done(String what) => showOcToast(context, what);
  return showOcMenu(
    context: context,
    position: Offset(size.width / 2 - 120, size.height / 3),
    entries: [
      if (mock != null && server != null) ...[
        OcMenuItem(
          label: 'Disconnect this server',
          icon: OcIcons.cloudOff,
          onSelected: () => mock.debugDisconnect(server),
        ),
        OcMenuItem(
          label: 'Reconnect this server',
          icon: OcIcons.refresh,
          onSelected: () => mock.debugReconnect(server),
        ),
        OcMenuItem(
          label: 'Fingerprint mismatch',
          icon: OcIcons.fingerprint,
          onSelected: () => mock.debugFingerprintMismatch(server),
        ),
        OcMenuItem(
          label: 'Add a 10 000-message channel',
          icon: OcIcons.forum,
          onSelected: () {
            mock.debugStressChannel(server);
            done('Added #scroll-test');
          },
        ),
      ],
      if (mock != null) ...[
        OcMenuItem(
          label: 'Long channel names',
          icon: OcIcons.tag,
          onSelected: mock.debugLongNames,
        ),
        OcMenuItem(
          label: 'Empty server list',
          icon: OcIcons.dns,
          onSelected: mock.debugEmptyServerList,
        ),
        OcMenuItem(
          label: 'Reset mock data',
          icon: OcIcons.refresh,
          onSelected: mock.debugReset,
        ),
        const OcMenuDivider(),
      ],
      OcMenuItem(
        label: 'Widget gallery',
        icon: OcIcons.palette,
        onSelected: () => context.go('/gallery'),
      ),
    ],
  );
}
