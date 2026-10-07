import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// The identity lives in the system keyring; when that cannot be reached
/// (none on the bus, or its unlock prompt dismissed), nothing else can
/// start. Says why, and tries again once the keyring is there.
class KeyringUnavailableView extends ConsumerStatefulWidget {
  const KeyringUnavailableView({super.key, required this.problem});

  final RepoException problem;

  @override
  ConsumerState<KeyringUnavailableView> createState() =>
      _KeyringUnavailableViewState();
}

class _KeyringUnavailableViewState
    extends ConsumerState<KeyringUnavailableView> {
  var _busy = false;

  Future<void> _retry() async {
    setState(() => _busy = true);
    try {
      await ref.read(repositoryProvider).reloadIdentity();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String get _hint => switch (defaultTargetPlatform) {
    TargetPlatform.linux =>
      'Start or unlock a keyring that offers the Secret Service, such as '
          'GNOME Keyring, KeePassXC or KWallet, then try again.',
    TargetPlatform.macOS => 'Unlock your login keychain, then try again.',
    _ => 'Make sure the system keyring is available, then try again.',
  };

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Material(
      color: colors.chat,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const HeaderBar(
            leadingControls: true,
            trailingControls: true,
            child: SizedBox(),
          ),
          Expanded(
            child: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(OcSpace.s24),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 440),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Icon(OcIcons.key, size: 48, color: colors.text),
                      const SizedBox(height: OcSpace.s16),
                      Text(
                        "Opencord can't reach your keyring",
                        textAlign: TextAlign.center,
                        style: OcText.title.copyWith(color: colors.text),
                      ),
                      const SizedBox(height: OcSpace.s8),
                      SelectableText(
                        widget.problem.message,
                        textAlign: TextAlign.center,
                        style: OcText.body.copyWith(
                          color: colors.textSecondary,
                        ),
                      ),
                      const SizedBox(height: OcSpace.s8),
                      Text(
                        _hint,
                        textAlign: TextAlign.center,
                        style: OcText.body.copyWith(
                          color: colors.textSecondary,
                        ),
                      ),
                      const SizedBox(height: OcSpace.s24),
                      OcButton.primary(
                        label: 'Try again',
                        expand: true,
                        busy: _busy,
                        onPressed: _busy ? null : _retry,
                      ),
                      const SizedBox(height: OcSpace.s8),
                      OcButton(
                        label: 'Quit',
                        expand: true,
                        onPressed: () => ref.read(nativeWindowProvider).quit(),
                      ),
                    ],
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
