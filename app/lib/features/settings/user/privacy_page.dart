import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// The privacy settings page (§6.5).
class PrivacyPage extends ConsumerStatefulWidget {
  const PrivacyPage({super.key});

  @override
  ConsumerState<PrivacyPage> createState() => _PrivacyPageState();
}

class _PrivacyPageState extends ConsumerState<PrivacyPage> with SaveBarChanges {
  PrivacySettings _saved = const PrivacySettings();
  late PrivacySettings _draft = _saved;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final settings = await ref.read(repositoryProvider).privacySettings();
      if (!mounted) return;
      setState(() {
        if (!unsavedChanges) {
          _draft = settings;
        }
        _saved = settings;
      });
    } on RepoException catch (_) {
      // Ignored: keep defaults.
    }
  }

  @override
  bool get unsavedChanges => _draft != _saved;

  @override
  Future<void> saveChanges() async {
    final toSave = _draft;
    await ref.read(repositoryProvider).savePrivacySettings(toSave);
    _saved = toSave;
  }

  @override
  void discardChanges() {
    if (!mounted) return;
    setState(() {
      _draft = _saved;
    });
  }

  @override
  Widget build(BuildContext context) {
    final unavailable = ref.watch(repositoryProvider).privateUnavailable;
    final enabled = unavailable == null;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (unavailable != null)
          Padding(
            padding: const EdgeInsets.only(bottom: OcSpace.s16),
            child: InlineError(unavailable.message),
          ),
        SettingsSection(
          children: [
            SettingsSwitchRow(
              title: 'Friend requests',
              subtitle: 'Let people send you friend requests.',
              value: _draft.allowFriendRequests,
              onChanged: enabled
                  ? (value) {
                      setState(() {
                        _draft = _draft.copyWith(allowFriendRequests: value);
                      });
                      changesEdited();
                    }
                  : null,
            ),
            SettingsSwitchRow(
              title: 'Message requests',
              subtitle: "Let people who aren't your friends message you.",
              value: _draft.allowMessageRequests,
              onChanged: enabled
                  ? (value) {
                      setState(() {
                        _draft = _draft.copyWith(allowMessageRequests: value);
                      });
                      changesEdited();
                    }
                  : null,
            ),
          ],
        ),
      ],
    );
  }
}
