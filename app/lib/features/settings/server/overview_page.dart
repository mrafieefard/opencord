import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Server overview (§8.2): name, description, who may join, and the
/// address and fingerprint to share.
class ServerOverviewPage extends ConsumerStatefulWidget {
  const ServerOverviewPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerOverviewPage> createState() => _ServerOverviewPageState();
}

class _ServerOverviewPageState extends ConsumerState<ServerOverviewPage> {
  late final TextEditingController _name;
  late final TextEditingController _description;
  late bool _openJoin;
  var _saving = false;
  String? _error;

  ServerInfo? get _info =>
      ref.read(serverProvider(widget.serverKey)).data?.info;

  @override
  void initState() {
    super.initState();
    final info = _info;
    _name = TextEditingController(text: info?.name ?? '')
      ..addListener(() => setState(() {}));
    _description = TextEditingController(text: info?.description ?? '')
      ..addListener(() => setState(() {}));
    _openJoin = info?.openJoin ?? false;
  }

  @override
  void dispose() {
    _name.dispose();
    _description.dispose();
    super.dispose();
  }

  bool get _changed {
    final info = _info;
    if (info == null) return false;
    return _name.text.trim() != info.name ||
        _description.text.trim() != info.description ||
        _openJoin != info.openJoin;
  }

  Future<void> _save() async {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'The server needs a name.');
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(repositoryProvider)
          .updateServer(
            widget.serverKey,
            name: name,
            description: _description.text.trim(),
            openJoin: _openJoin,
          );
      if (mounted) showOcToast(context, 'Changes saved');
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final summary = ref
        .watch(serverListProvider)
        .where((server) => server.key == widget.serverKey)
        .firstOrNull;
    final fingerprint = summary?.fingerprint;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Server',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text('Name', style: OcText.body.copyWith(color: colors.text)),
                  const SizedBox(height: OcSpace.s8),
                  OcTextField(
                    controller: _name,
                    semanticLabel: 'Server name',
                    maxLength: 100,
                  ),
                  const SizedBox(height: OcSpace.s16),
                  Text(
                    'Description',
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                  const SizedBox(height: OcSpace.s8),
                  OcTextField(
                    controller: _description,
                    semanticLabel: 'Server description',
                    hint: 'What this server is about',
                    minLines: 2,
                    maxLines: 4,
                    maxLength: 500,
                  ),
                ],
              ),
            ),
            SettingsSwitchRow(
              title: 'Open to anyone with the address',
              subtitle: 'Off: people need an invite to join',
              value: _openJoin,
              onChanged: (value) => setState(() => _openJoin = value),
            ),
          ],
        ),
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s12),
          InlineError(error),
        ],
        const SizedBox(height: OcSpace.s16),
        Align(
          alignment: Alignment.centerRight,
          child: OcButton.primary(
            label: 'Save changes',
            busy: _saving,
            onPressed: _changed ? _save : null,
          ),
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Address',
          footer:
              'Share both: people compare the fingerprint when they add the '
              'server by its address. Invite links carry it for them.',
          children: [
            SettingsRow(
              title: 'Address',
              subtitle: widget.serverKey,
              mono: true,
            ),
            SettingsRow(
              title: 'Fingerprint',
              subtitle: fingerprint == null
                  ? 'A public certificate vouches for this server'
                  : groupedFingerprint(fingerprint),
              mono: fingerprint != null,
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        Text(
          'Server icons arrive with file uploads in a later version.',
          style: OcText.small.copyWith(color: colors.textMuted),
        ),
      ],
    );
  }
}
