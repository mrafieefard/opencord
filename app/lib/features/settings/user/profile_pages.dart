import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// My profile (§8.1): avatar, display name and presence.
class ProfilePage extends ConsumerStatefulWidget {
  const ProfilePage({super.key});

  @override
  ConsumerState<ProfilePage> createState() => _ProfilePageState();
}

class _ProfilePageState extends ConsumerState<ProfilePage> {
  Map<Object, bool Function()>? _unsaved;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved = SettingsScope.of(context)
      ?..[this] = () =>
          _name.text.trim().isNotEmpty && _name.text.trim() != _saved;
  }

  late final TextEditingController _name;
  var _saving = false;
  String? _error;

  String get _saved => ref.read(repositoryProvider).identity?.displayName ?? '';

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(text: _saved)
      ..addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _unsaved?.remove(this);
    _name.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final name = _name.text.trim();
    if (name.isEmpty || name == _saved) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref.read(repositoryProvider).updateDisplayName(name);
      if (mounted) showOcToast(context, 'Display name saved');
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final identity = ref.read(repositoryProvider).identity;
    final presence = ref.watch(selfPresenceProvider);
    final changed = _name.text.trim().isNotEmpty && _name.text.trim() != _saved;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Profile',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s16),
              child: Row(
                children: [
                  OcAvatar(
                    id: identity?.fingerprint ?? 'me',
                    name: _name.text.isEmpty ? '?' : _name.text,
                    size: 64,
                  ),
                  const SizedBox(width: OcSpace.s16),
                  Expanded(
                    child: Text(
                      'Avatars arrive with file uploads in a later version.',
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(OcSpace.s16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'Display name',
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                  const SizedBox(height: OcSpace.s8),
                  Row(
                    children: [
                      Expanded(
                        child: OcTextField(
                          controller: _name,
                          semanticLabel: 'Display name',
                          maxLength: 32,
                          onSubmitted: (_) => _save(),
                        ),
                      ),
                      const SizedBox(width: OcSpace.s8),
                      OcButton.primary(
                        label: 'Save',
                        busy: _saving,
                        onPressed: changed ? _save : null,
                      ),
                    ],
                  ),
                  if (_error case final error?) ...[
                    const SizedBox(height: OcSpace.s8),
                    InlineError(error),
                  ],
                  const SizedBox(height: OcSpace.s6),
                  Text(
                    'Shown on every server, unless you set a nickname there.',
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Presence',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s12),
              child: SettingsChoiceCards<SelfPresence>(
                value: presence,
                onChanged: ref.read(selfPresenceProvider.notifier).choose,
                options: [
                  for (final (option, description) in const [
                    (SelfPresence.online, 'Shown as online'),
                    (SelfPresence.idle, 'Away for now'),
                    (SelfPresence.doNotDisturb, 'No notifications'),
                    (SelfPresence.invisible, 'Shown as offline'),
                  ])
                    ChoiceCardOption(
                      value: option,
                      label: option.label,
                      description: description,
                      preview: PresenceBadge(
                        presence: option.shown,
                        size: 14,
                        ringColor: colors.surface,
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ],
    );
  }
}

/// Identity & keys (§8.1): the fingerprint, the public key and the backup.
class IdentityPage extends ConsumerWidget {
  const IdentityPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final identity = ref.read(repositoryProvider).identity;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Your identity',
          footer:
              'Servers know you by this key, not by a password. Compare the '
              'fingerprint when someone asks who you are.',
          children: [
            SettingsRow(
              title: 'Fingerprint',
              subtitle: identity?.fingerprint.replaceAll('-', ' ') ?? 'None',
              mono: true,
              trailing: OcButton(
                label: 'Copy public key',
                icon: OcIcons.contentCopy,
                dense: true,
                onPressed: identity == null || identity.publicKeyHex.isEmpty
                    ? null
                    : () {
                        Clipboard.setData(
                          ClipboardData(text: identity.publicKeyHex),
                        );
                        showOcToast(context, 'Public key copied');
                      },
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Backup',
          children: [
            SettingsRow(
              title: 'Export identity backup',
              subtitle: 'Keep a copy somewhere safe and private',
              icon: OcIcons.download,
              onTap: () => _export(context, ref),
            ),
            SettingsRow(
              title: 'Import identity',
              subtitle: 'Become the identity in a backup on this device',
              icon: OcIcons.upload,
              onTap: () => _import(context, ref),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        Text(
          'Anyone with your backup can be you on every server. Never share '
          'it. Without it, losing this device means losing this identity.',
          style: OcText.small.copyWith(color: colors.textSecondary),
        ),
      ],
    );
  }

  Future<void> _export(BuildContext context, WidgetRef ref) async {
    String backup;
    try {
      backup = await ref.read(repositoryProvider).exportIdentityBackup();
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
      return;
    }
    if (!context.mounted) return;
    await showOcDialog<void>(
      context: context,
      builder: (context) => _BackupDialog(backup: backup),
    );
  }

  Future<void> _import(BuildContext context, WidgetRef ref) async {
    final confirmed = await confirmAction(
      context,
      title: 'Replace this identity?',
      message:
          'Servers will see you as the identity in the backup. Export the '
          'current one first if you might want it back.',
      action: 'Continue',
    );
    if (!confirmed || !context.mounted) return;
    await showOcDialog<void>(
      context: context,
      builder: (context) => const _ImportDialog(),
    );
  }
}

class _BackupDialog extends StatelessWidget {
  const _BackupDialog({required this.backup});

  final String backup;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: 'Identity backup',
      width: 520,
      actions: [
        OcButton(label: 'Done', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Copy',
          icon: OcIcons.contentCopy,
          onPressed: () {
            Clipboard.setData(ClipboardData(text: backup));
            showOcToast(context, 'Backup copied');
          },
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text(
            'Store this in a password manager or on paper. Anyone who has '
            'it can be you.',
          ),
          const SizedBox(height: OcSpace.s12),
          Container(
            padding: const EdgeInsets.all(OcSpace.s12),
            decoration: BoxDecoration(
              color: colors.chat,
              borderRadius: BorderRadius.circular(OcRadius.input),
              border: Border.all(color: colors.border),
            ),
            child: SelectableText(
              backup,
              style: OcText.mono.copyWith(color: colors.text),
            ),
          ),
        ],
      ),
    );
  }
}

class _ImportDialog extends ConsumerStatefulWidget {
  const _ImportDialog();

  @override
  ConsumerState<_ImportDialog> createState() => _ImportDialogState();
}

class _ImportDialogState extends ConsumerState<_ImportDialog> {
  final _backup = TextEditingController();
  var _busy = false;
  String? _error;

  @override
  void dispose() {
    _backup.dispose();
    super.dispose();
  }

  Future<void> _import() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref.read(repositoryProvider).importIdentityBackup(_backup.text);
      if (!mounted) return;
      showOcToast(context, 'Identity imported');
      Navigator.pop(context);
    } on RepoException catch (error) {
      if (mounted) {
        setState(() {
          _error = error.message;
          _busy = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) => OcDialog(
    title: 'Import identity',
    width: 520,
    actions: [
      OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
      OcButton.primary(label: 'Import', busy: _busy, onPressed: _import),
    ],
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        const Text('Paste the backup you exported before.'),
        const SizedBox(height: OcSpace.s12),
        OcTextField(
          controller: _backup,
          autofocus: true,
          mono: true,
          maxLines: 3,
          minLines: 2,
          semanticLabel: 'Identity backup',
        ),
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s10),
          InlineError(error),
        ],
      ],
    ),
  );
}
