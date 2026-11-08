import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The dev tokens section of a plugin's page (§17.3).
class DevTokensSection extends ConsumerStatefulWidget {
  const DevTokensSection({
    super.key,
    required this.serverKey,
    required this.pluginId,
  });

  final String serverKey;
  final String pluginId;

  @override
  ConsumerState<DevTokensSection> createState() => _DevTokensSectionState();
}

class _DevTokensSectionState extends ConsumerState<DevTokensSection> {
  List<PluginDevToken> _tokens = [];
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final all = await ref
          .read(repositoryProvider)
          .fetchPluginDevTokens(widget.serverKey);
      if (mounted) {
        setState(() {
          _tokens = all.where((t) => t.pluginId == widget.pluginId).toList();
          _loading = false;
        });
      }
    } catch (_) {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _create() async {
    final created = await showCreateDevTokenDialog(
      context,
      serverKey: widget.serverKey,
      pluginId: widget.pluginId,
    );
    if (created && mounted) {
      await _load();
    }
  }

  Future<void> _revoke(int tokenId) async {
    try {
      await ref
          .read(repositoryProvider)
          .revokePluginDevToken(widget.serverKey, tokenId);
      if (mounted) {
        showOcToast(context, 'Dev token revoked');
        await _load();
      }
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    } catch (e) {
      if (mounted) showOcToast(context, e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionLabel(
          'Dev tokens',
          trailing: OcButton(
            label: 'Create a dev token',
            icon: OcIcons.add,
            dense: true,
            onPressed: _create,
          ),
        ),
        const SizedBox(height: OcSpace.s12),
        if (_loading)
          const Center(
            child: Padding(
              padding: EdgeInsets.all(OcSpace.s20),
              child: SizedBox(
                width: 24,
                height: 24,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
          )
        else if (_tokens.isEmpty)
          Container(
            padding: const EdgeInsets.all(OcSpace.s24),
            decoration: BoxDecoration(
              color: colors.surface,
              borderRadius: BorderRadius.circular(OcRadius.section),
              border: Border.all(color: colors.border),
            ),
            alignment: Alignment.center,
            child: Text(
              'No dev tokens yet.',
              style: OcText.body.copyWith(color: colors.textMuted),
            ),
          )
        else
          SettingsSection(
            children: [
              for (final token in _tokens)
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: OcSpace.s16,
                    vertical: OcSpace.s12,
                  ),
                  child: Row(
                    children: [
                      Icon(
                        OcIcons.fingerprint,
                        size: 20,
                        color: colors.textSecondary,
                      ),
                      const SizedBox(width: OcSpace.s12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              'Expires: ${longDate(token.expiresAt)}',
                              style: OcText.body.copyWith(
                                color: colors.text,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(height: 2),
                            Text(
                              'Created: ${longDate(token.createdAt)} • Last used: ${token.lastUsedAt != null ? longDate(token.lastUsedAt!) : 'Never'}',
                              style: OcText.small.copyWith(
                                color: colors.textMuted,
                              ),
                            ),
                          ],
                        ),
                      ),
                      OcButton.ghost(
                        label: 'Revoke',
                        dense: true,
                        onPressed: () => _revoke(token.id),
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

/// Opens the create dev token dialog (§17.3).
Future<bool> showCreateDevTokenDialog(
  BuildContext context, {
  required String serverKey,
  required String pluginId,
}) async {
  final res = await showOcDialog<bool>(
    context: context,
    builder: (context) =>
        _CreateDevTokenDialog(serverKey: serverKey, pluginId: pluginId),
  );
  return res ?? false;
}

class _CreateDevTokenDialog extends ConsumerStatefulWidget {
  const _CreateDevTokenDialog({
    required this.serverKey,
    required this.pluginId,
  });

  final String serverKey;
  final String pluginId;

  @override
  ConsumerState<_CreateDevTokenDialog> createState() =>
      _CreateDevTokenDialogState();
}

class _CreateDevTokenDialogState extends ConsumerState<_CreateDevTokenDialog> {
  final _daysController = TextEditingController(text: '7');
  bool _busy = false;
  String? _error;
  CreatedPluginDevToken? _created;

  @override
  void dispose() {
    _daysController.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final days = int.tryParse(_daysController.text.trim()) ?? 7;
    final clampedDays = days.clamp(1, 30);

    setState(() {
      _busy = true;
      _error = null;
    });

    try {
      final res = await ref
          .read(repositoryProvider)
          .createPluginDevToken(
            widget.serverKey,
            widget.pluginId,
            days: clampedDays,
          );
      if (mounted) {
        setState(() {
          _created = res;
          _busy = false;
        });
      }
    } on RepoException catch (e) {
      if (mounted) {
        setState(() {
          _error = e.message;
          _busy = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = e.toString();
          _busy = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final created = _created;

    if (created != null) {
      final address = widget.serverKey;
      final cmd =
          'opencord-plugin dev --server $address --fingerprint ${created.serverFingerprint} --token ${created.secret}';

      return OcDialog(
        title: 'Dev token created',
        width: 500,
        actions: [
          OcButton.primary(
            label: 'Done',
            onPressed: () => Navigator.of(context).pop(true),
          ),
        ],
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'This secret will never be shown again.',
              style: OcText.body.copyWith(color: colors.textSecondary),
            ),
            const SizedBox(height: OcSpace.s12),
            Container(
              padding: const EdgeInsets.symmetric(
                horizontal: OcSpace.s12,
                vertical: OcSpace.s8,
              ),
              decoration: BoxDecoration(
                color: colors.hover,
                borderRadius: BorderRadius.circular(OcRadius.section),
                border: Border.all(color: colors.border),
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      created.secret,
                      style: OcText.mono.copyWith(color: colors.text),
                    ),
                  ),
                  OcButton.ghost(
                    label: 'Copy',
                    dense: true,
                    onPressed: () {
                      Clipboard.setData(ClipboardData(text: created.secret));
                      showOcToast(context, 'Secret copied');
                    },
                  ),
                ],
              ),
            ),
            const SizedBox(height: OcSpace.s16),
            Text(
              'Run in your plugin folder:',
              style: OcText.small.copyWith(
                color: colors.textSecondary,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: OcSpace.s6),
            Container(
              padding: const EdgeInsets.all(OcSpace.s12),
              decoration: BoxDecoration(
                color: colors.hover,
                borderRadius: BorderRadius.circular(OcRadius.section),
                border: Border.all(color: colors.border),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SelectableText(
                    cmd,
                    style: OcText.mono.copyWith(
                      color: colors.text,
                      fontSize: 12,
                    ),
                  ),
                  const SizedBox(height: OcSpace.s8),
                  Align(
                    alignment: Alignment.centerRight,
                    child: OcButton.ghost(
                      label: 'Copy',
                      dense: true,
                      onPressed: () {
                        Clipboard.setData(ClipboardData(text: cmd));
                        showOcToast(context, 'Command copied');
                      },
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      );
    }

    return OcDialog(
      title: 'Create dev token',
      width: 440,
      actions: [
        OcButton.ghost(
          label: 'Cancel',
          onPressed: _busy ? null : () => Navigator.of(context).pop(false),
        ),
        OcButton.primary(label: 'Create', busy: _busy, onPressed: _submit),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            'Days (1–30)',
            style: OcText.small.copyWith(
              color: colors.textSecondary,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: OcSpace.s6),
          TextField(
            controller: _daysController,
            keyboardType: TextInputType.number,
            inputFormatters: [FilteringTextInputFormatter.digitsOnly],
            decoration: ocInputDecoration(context, hint: '7'),
          ),
          if (_error != null) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(_error!),
          ],
        ],
      ),
    );
  }
}
