import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// Server settings → Private messages: whether members can message each
/// other privately through this server, and whether only friends can.
class ServerPrivateMessagesPage extends ConsumerStatefulWidget {
  const ServerPrivateMessagesPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerPrivateMessagesPage> createState() =>
      _ServerPrivateMessagesPageState();
}

class _ServerPrivateMessagesPageState
    extends ConsumerState<ServerPrivateMessagesPage>
    with SaveBarChanges {
  /// As saved; kept here for the save bar (see [SaveBarChanges]).
  late ServerPrivateSettings _saved = _read();
  late ServerPrivateSettings _draft = _saved;

  ServerPrivateSettings _read() =>
      ref.read(serverProvider(widget.serverKey)).data?.privateSettings ??
      const ServerPrivateSettings();

  @override
  bool get unsavedChanges => _draft != _saved;

  @override
  Future<void> saveChanges() async {
    final draft = _draft;
    _saved = await ref
        .read(repositoryProvider)
        .updatePrivateSettings(
          widget.serverKey,
          enabled: draft.enabled,
          friendsOnly: draft.friendsOnly,
        );
  }

  @override
  void discardChanges() {
    if (!mounted) return;
    setState(() => _draft = _saved);
  }

  void _edit(ServerPrivateSettings draft) {
    setState(() => _draft = draft);
    changesEdited();
  }

  @override
  Widget build(BuildContext context) {
    // A change from elsewhere shows at once unless something is being
    // edited here.
    ref.listen(
      serverProvider(
        widget.serverKey,
      ).select((state) => state.data?.privateSettings),
      (_, settings) {
        if (settings == null || settings == _saved) return;
        setState(() {
          if (!unsavedChanges) _draft = settings;
          _saved = settings;
        });
      },
    );
    final allowed = _saved.allowedByConfig;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!allowed)
          const Padding(
            padding: EdgeInsets.only(bottom: OcSpace.s16),
            child: InlineError(
              "Private messages are turned off in this server's config file "
              '([private_messages] enabled = false).',
            ),
          ),
        SettingsSection(
          children: [
            SettingsSwitchRow(
              title: 'Private messages',
              subtitle:
                  'Members can message each other privately through this '
                  "server. Messages are end-to-end encrypted: the server can't "
                  'read them.',
              value: allowed && _draft.enabled,
              onChanged: allowed
                  ? (value) => _edit(
                      ServerPrivateSettings(
                        enabled: value,
                        friendsOnly: _draft.friendsOnly,
                        allowedByConfig: allowed,
                      ),
                    )
                  : null,
            ),
            SettingsSwitchRow(
              title: 'Only between friends',
              subtitle:
                  'Messages go only between two members who are each '
                  "other's friends; friend requests still go through. Their "
                  'apps tell this server who their friends are.',
              value: _draft.friendsOnly,
              onChanged: allowed && _draft.enabled
                  ? (value) => _edit(
                      ServerPrivateSettings(
                        enabled: _draft.enabled,
                        friendsOnly: value,
                        allowedByConfig: allowed,
                      ),
                    )
                  : null,
            ),
          ],
        ),
      ],
    );
  }
}
