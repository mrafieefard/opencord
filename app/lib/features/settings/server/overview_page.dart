import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/settings/picture_chooser.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/settings.dart';

/// Server overview (§8.2): icon, name, description, who may join, and the
/// address and fingerprint to share. Changes wait for the save bar.
class ServerOverviewPage extends ConsumerStatefulWidget {
  const ServerOverviewPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerOverviewPage> createState() => _ServerOverviewPageState();
}

class _ServerOverviewPageState extends ConsumerState<ServerOverviewPage> {
  SettingsChanges? _changes;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _changes = SettingsScope.changesOf(context)
      ?..register(
        this,
        PendingChanges(
          dirty: () => _changed || _iconChanged,
          save: _save,
          discard: _discard,
        ),
      );
  }

  late final TextEditingController _name;
  late final TextEditingController _description;
  late bool _openJoin;
  String? _error;

  /// The info as saved; kept here, as the save bar may ask while this page
  /// is going away, when `ref` can no longer be used.
  late ServerInfo? _info = ref
      .read(serverProvider(widget.serverKey))
      .data
      ?.info;

  /// An icon chosen or removed here, not saved yet.
  String? _chosenIcon;
  var _iconRemoved = false;

  bool get _iconChanged =>
      _chosenIcon != null || (_iconRemoved && _info?.iconHash != null);

  void _chooseIcon(String? path) {
    setState(() {
      _chosenIcon = path;
      _iconRemoved = path == null;
    });
    _changes?.touch();
  }

  void _discard() {
    final info = _info;
    _name.text = info?.name ?? '';
    _description.text = info?.description ?? '';
    if (mounted) {
      setState(() {
        _openJoin = info?.openJoin ?? false;
        _chosenIcon = null;
        _iconRemoved = false;
      });
    }
  }

  @override
  void initState() {
    super.initState();
    final info = _info;
    void edited() {
      setState(() {});
      _changes?.touch();
    }

    _name = TextEditingController(text: info?.name ?? '')..addListener(edited);
    _description = TextEditingController(text: info?.description ?? '')
      ..addListener(edited);
    _openJoin = info?.openJoin ?? false;
  }

  @override
  void dispose() {
    _changes?.unregister(this);
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
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'The server needs a name.',
      );
    }
    setState(() => _error = null);
    final repository = ref.read(repositoryProvider);
    try {
      if (_changed) {
        await repository.updateServer(
          widget.serverKey,
          name: name,
          description: _description.text.trim(),
          openJoin: _openJoin,
        );
      }
      if (_iconChanged) {
        await repository.setServerIcon(
          widget.serverKey,
          _iconRemoved ? null : _chosenIcon,
        );
        if (mounted) {
          setState(() {
            _chosenIcon = null;
            _iconRemoved = false;
          });
        }
      }
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
      rethrow;
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final info = ref.watch(
      serverProvider(widget.serverKey).select((state) => state.data?.info),
    );
    _info = info;
    final savedIcon = avatarImage(ref, widget.serverKey, info?.iconHash);
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
              child: PictureChooser(
                id: widget.serverKey,
                name: _name.text,
                square: true,
                image: switch ((_chosenIcon, _iconRemoved)) {
                  (final chosen?, _) => FileImage(File(chosen)),
                  (_, true) => null,
                  _ => savedIcon,
                },
                onPicked: _chooseIcon,
                onRemoved:
                    _chosenIcon != null || (savedIcon != null && !_iconRemoved)
                    ? () => _chooseIcon(null)
                    : null,
              ),
            ),
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
              onChanged: (value) {
                setState(() => _openJoin = value);
                _changes?.touch();
              },
            ),
          ],
        ),
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s12),
          InlineError(error),
        ],
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
      ],
    );
  }
}
