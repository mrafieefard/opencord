import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/encryption_dialog.dart';
import 'package:opencord/features/home/key_banners.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Keep new keys was tapped: until the app starts again.
class KeepingNewKeys extends Notifier<bool> {
  @override
  bool build() => false;

  void keep() => state = true;
}

final keepingNewKeysProvider = NotifierProvider<KeepingNewKeys, bool>(
  KeepingNewKeys.new,
);

/// In the home view: this person's other devices with other chat keys
/// (design 2026-11-01 §7.1).
class OwnDeviceNotice extends ConsumerWidget {
  const OwnDeviceNotice({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final own = ref.watch(ownDevicesProvider);
    final keepingNewKeys = ref.watch(keepingNewKeysProvider);
    final others = own.devices
        .where((device) => device.state == OwnDeviceState.other)
        .toList();
    final offering = own.devices
        .where((device) => device.state == OwnDeviceState.offering)
        .toList();
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final device in offering)
          KeyBanner(
            icon: OcIcons.key,
            message:
                '${deviceLabel(device.deviceId)} wants to give this device '
                'its keys.',
            actions: [
              OcButton.primary(
                label: 'Compare words',
                onPressed: () => showPairingDialog(
                  context,
                  ref,
                  deviceId: device.deviceId,
                  offering: true,
                ),
              ),
            ],
          ),
        if (!own.hasChats && others.isNotEmpty && !keepingNewKeys)
          KeyBanner(
            icon: OcIcons.key,
            message:
                'Your other devices use different keys. Approve this device '
                'on one of them so they share your chats.',
            actions: [
              OcButton(
                label: 'Keep new keys',
                onPressed: ref.read(keepingNewKeysProvider.notifier).keep,
              ),
            ],
          ),
        if (own.hasChats)
          for (final device in others)
            KeyBanner(
              icon: OcIcons.warning,
              warning: true,
              message:
                  'A device signed in as you with different keys. Approve it '
                  "only if it's yours.",
              actions: [
                OcButton.primary(
                  label: 'Approve',
                  onPressed: () => showPairingDialog(
                    context,
                    ref,
                    deviceId: device.deviceId,
                    offering: false,
                  ),
                ),
                OcButton(
                  label: 'Not mine',
                  onPressed: () async {
                    try {
                      await ref
                          .read(repositoryProvider)
                          .rejectOwnDevice(device.deviceId);
                    } on RepoException catch (e) {
                      if (!context.mounted) return;
                      await showPrivateFailure(
                        context,
                        e,
                        name: deviceLabel(device.deviceId),
                      );
                    }
                  },
                ),
              ],
            ),
      ],
    );
  }
}
