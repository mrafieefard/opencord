import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/providers/plugins_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/plugins/plugin_config_fields.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// Opens the install or update review dialog (§9.1, §9.2, §17.4).
Future<void> showReviewDialog(
  BuildContext context, {
  required String serverKey,
  PluginReview? review,
  Uint8List? uploadBytes,
  String? fileName,
}) {
  return showOcDialog<void>(
    context: context,
    builder: (context) => ReviewDialog(
      serverKey: serverKey,
      review: review,
      uploadBytes: uploadBytes,
      fileName: fileName,
    ),
  );
}

class ReviewDialog extends ConsumerStatefulWidget {
  const ReviewDialog({
    super.key,
    required this.serverKey,
    this.review,
    this.uploadBytes,
    this.fileName,
  });

  final String serverKey;
  final PluginReview? review;
  final Uint8List? uploadBytes;
  final String? fileName;

  @override
  ConsumerState<ReviewDialog> createState() => _ReviewDialogState();
}

class _ReviewDialogState extends ConsumerState<ReviewDialog> {
  PluginReview? _review;
  bool _busy = false;
  String? _error;
  bool _licenseAccepted = false;
  final Map<String, bool> _granted = {};
  final Map<String, String> _configValues = {};
  Timer? _expiryTimer;

  @override
  void initState() {
    super.initState();
    _review = widget.review;
    if (_review != null) {
      _initFromReview(_review!);
    } else if (widget.uploadBytes != null) {
      _upload();
    }
  }

  void _initFromReview(PluginReview rev) {
    for (final p in rev.plugin.permissions) {
      _granted[p.name] = p.grantable ? p.granted : false;
    }
    for (final field in rev.plugin.configFields) {
      final existing = rev.plugin.config.where((c) => c.key == field.key);
      _configValues[field.key] = existing.isNotEmpty
          ? existing.first.value
          : '';
    }
    _armExpiry(rev);
  }

  void _armExpiry(PluginReview rev) {
    _expiryTimer?.cancel();
    final remaining = rev.expiresAt.difference(DateTime.now());
    if (remaining.isNegative) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).maybePop();
      });
    } else {
      _expiryTimer = Timer(remaining, () {
        if (mounted) Navigator.of(context).maybePop();
      });
    }
  }

  @override
  void dispose() {
    _expiryTimer?.cancel();
    super.dispose();
  }

  Future<void> _upload() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final rev = await ref
          .read(repositoryProvider)
          .uploadPluginPackage(widget.serverKey, widget.uploadBytes!);
      if (mounted) {
        setState(() {
          _review = rev;
          _busy = false;
        });
        _initFromReview(rev);
      }
    } on RepoException catch (e) {
      if (mounted) {
        setState(() {
          _error = e.message;
          _busy = false;
        });
      }
    }
  }

  Future<void> _confirm() async {
    final rev = _review;
    if (rev == null) return;

    // Check valid numbers
    for (final field in rev.plugin.configFields) {
      final val = _configValues[field.key]?.trim() ?? '';
      if (field.kind == PluginConfigKind.number &&
          val.isNotEmpty &&
          num.tryParse(val) == null) {
        setState(() => _error = '${field.label} must be a valid number');
        return;
      }
    }

    setState(() {
      _busy = true;
      _error = null;
    });

    final grantedList = <String>[
      for (final entry in _granted.entries)
        if (entry.value) entry.key,
    ];

    final configValues = <PluginConfigValue>[
      for (final field in rev.plugin.configFields)
        PluginConfigValue(
          key: field.key,
          value: _configValues[field.key] ?? '',
          hasValue: (_configValues[field.key] ?? '').isNotEmpty,
        ),
    ];

    try {
      await ref
          .read(repositoryProvider)
          .confirmPluginReview(
            widget.serverKey,
            rev.reviewId,
            grantedList,
            configValues,
            _licenseAccepted,
          );
      if (mounted) Navigator.of(context).pop();
    } on RepoException catch (e) {
      if (mounted) {
        setState(() {
          _error = e.message;
          _busy = false;
        });
      }
    }
  }

  Future<void> _discard() async {
    final rev = _review;
    if (rev != null) {
      try {
        await ref
            .read(repositoryProvider)
            .discardPluginReview(widget.serverKey, rev.reviewId);
      } on RepoException catch (e) {
        appLog('Failed to discard review: $e', name: 'plugins');
      }
    }
    if (mounted) Navigator.of(context).pop();
  }

  Widget _signatureBadge(OcColors colors, Plugin plugin) {
    if (plugin.githubCheck == GithubCheck.verified) {
      return Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
        decoration: BoxDecoration(
          color: colors.hover,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: colors.border),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(OcIcons.check, size: 12, color: colors.text),
            const SizedBox(width: 4),
            Text(
              'Verified',
              style: OcText.meta.copyWith(
                color: colors.text,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
      );
    }
    final label = switch (plugin.signature) {
      PluginSignature.signed => 'Signed',
      PluginSignature.dev => 'Dev',
      PluginSignature.unsigned => 'Unsigned',
    };
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: colors.hover,
        borderRadius: BorderRadius.circular(4),
        border: Border.all(color: colors.border),
      ),
      child: Text(
        label,
        style: OcText.meta.copyWith(
          color: colors.textMuted,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    // Clean close if the review was removed from provider (expired or confirmed elsewhere).
    ref.listen<Map<String, PluginReview>>(
      pluginReviewsProvider(widget.serverKey),
      (prev, next) {
        final current = _review;
        if (current != null &&
            prev?.containsKey(current.reviewId) == true &&
            !next.containsKey(current.reviewId)) {
          Navigator.of(context).maybePop();
        }
      },
    );

    final rev = _review;

    if (rev == null) {
      return OcDialog(
        title: 'Install plugin',
        width: 500,
        actions: [
          OcButton.ghost(
            label: 'Close',
            onPressed: () => Navigator.of(context).pop(),
          ),
        ],
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (_busy) ...[
              const Center(child: OcSpinner()),
              const SizedBox(height: OcSpace.s16),
              Center(
                child: Text(
                  'Inspecting plugin package...',
                  style: OcText.body.copyWith(color: colors.textSecondary),
                ),
              ),
            ] else if (_error != null) ...[
              InlineError(_error!),
            ],
          ],
        ),
      );
    }

    final plugin = rev.plugin;
    final isUpdate = rev.installed != null;
    final installed = rev.installed;

    final canSubmit =
        !_busy && (!rev.licenseNeedsAcceptance || _licenseAccepted);

    return OcDialog(
      title: isUpdate ? 'Update ${plugin.name}' : 'Install ${plugin.name}',
      width: 520,
      actions: [
        OcButton.ghost(label: 'Cancel', onPressed: _busy ? null : _discard),
        OcButton.primary(
          label: isUpdate ? 'Update' : 'Install',
          busy: _busy,
          onPressed: canSubmit ? _confirm : null,
        ),
      ],
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxHeight: 340),
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              // Header (§9.1): icon avatar, name, version, author, description, signature badge, licence
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: 44,
                    height: 44,
                    decoration: BoxDecoration(
                      color: colors.hover,
                      borderRadius: BorderRadius.circular(10),
                      border: Border.all(color: colors.border),
                    ),
                    alignment: Alignment.center,
                    child: Text(
                      plugin.name.isNotEmpty
                          ? plugin.name.substring(0, 1).toUpperCase()
                          : 'P',
                      style: OcText.title.copyWith(color: colors.text),
                    ),
                  ),
                  const SizedBox(width: OcSpace.s12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Row(
                          children: [
                            Flexible(
                              child: Text(
                                plugin.name,
                                style: OcText.title.copyWith(
                                  color: colors.text,
                                ),
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                            const SizedBox(width: OcSpace.s8),
                            Text(
                              'v${plugin.version}',
                              style: OcText.small.copyWith(
                                color: colors.textMuted,
                              ),
                            ),
                            const SizedBox(width: OcSpace.s8),
                            _signatureBadge(colors, plugin),
                          ],
                        ),
                        const SizedBox(height: 2),
                        Text(
                          'by ${plugin.author}${plugin.license.isNotEmpty ? ' · ${plugin.license}' : ''}',
                          style: OcText.small.copyWith(
                            color: colors.textSecondary,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
              if (plugin.description.isNotEmpty) ...[
                const SizedBox(height: OcSpace.s8),
                Text(
                  plugin.description,
                  style: OcText.body.copyWith(color: colors.textSecondary),
                ),
              ],
              const SizedBox(height: OcSpace.s12),

              if (isUpdate && installed != null) ...[
                Text(
                  'This replaces ${installed.name} ${installed.version} by ${installed.author}',
                  style: OcText.body.copyWith(color: colors.textSecondary),
                ),
                const SizedBox(height: OcSpace.s12),
              ],

              // Signature and GitHub lines
              if (plugin.githubCheck == GithubCheck.verified) ...[
                Row(
                  children: [
                    Icon(OcIcons.check, size: 16, color: colors.text),
                    const SizedBox(width: OcSpace.s6),
                    Expanded(
                      child: Text(
                        'Verified: github.com/${plugin.github}',
                        style: OcText.small.copyWith(color: colors.text),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: OcSpace.s8),
              ] else if (plugin.githubCheck == GithubCheck.mismatch) ...[
                Row(
                  children: [
                    Icon(OcIcons.error, size: 16, color: colors.text),
                    const SizedBox(width: OcSpace.s6),
                    Expanded(
                      child: Text(
                        'GitHub publishes a different key',
                        style: OcText.small.copyWith(color: colors.text),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: OcSpace.s8),
              ] else if (plugin.githubCheck == GithubCheck.failed) ...[
                Row(
                  children: [
                    Icon(OcIcons.error, size: 16, color: colors.textMuted),
                    const SizedBox(width: OcSpace.s6),
                    Expanded(
                      child: Text(
                        "Couldn't check GitHub",
                        style: OcText.small.copyWith(color: colors.textMuted),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: OcSpace.s8),
              ],

              if (plugin.signature == PluginSignature.unsigned) ...[
                Container(
                  padding: const EdgeInsets.all(OcSpace.s10),
                  decoration: BoxDecoration(
                    color: colors.hover,
                    borderRadius: BorderRadius.circular(OcRadius.section),
                    border: Border.all(color: colors.border),
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Icon(
                          OcIcons.error,
                          size: 16,
                          color: colors.text,
                        ),
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Text(
                          'Unsigned: anyone could have changed this plugin. Install it only if you trust where it came from.',
                          style: OcText.small.copyWith(color: colors.text),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: OcSpace.s12),
              ],

              // Permissions section
              if (plugin.permissions.isNotEmpty) ...[
                const SectionLabel('Permissions'),
                const SizedBox(height: OcSpace.s6),
                for (final p in plugin.permissions) ...[
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Row(
                                children: [
                                  Text(
                                    p.name,
                                    style: OcText.body.copyWith(
                                      color: p.grantable
                                          ? colors.text
                                          : colors.textMuted,
                                    ),
                                  ),
                                  if (isUpdate &&
                                      rev.newPermissions.contains(p.name)) ...[
                                    const SizedBox(width: OcSpace.s6),
                                    const NewBadge(),
                                  ],
                                ],
                              ),
                              if (!p.grantable) ...[
                                const SizedBox(height: 2),
                                Text(
                                  "You don't have this permission yourself",
                                  style: OcText.small.copyWith(
                                    color: colors.textMuted,
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),
                        OcSwitch(
                          value: p.grantable
                              ? (_granted[p.name] ?? false)
                              : false,
                          onChanged: p.grantable
                              ? (val) => setState(() => _granted[p.name] = val)
                              : null,
                        ),
                      ],
                    ),
                  ),
                  Divider(height: 1, color: colors.border),
                ],
                const SizedBox(height: OcSpace.s12),
              ],

              // Domains section
              if (plugin.domains.isNotEmpty) ...[
                const SectionLabel('Domains'),
                const SizedBox(height: OcSpace.s6),
                for (final domain in plugin.domains)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 2),
                    child: Row(
                      children: [
                        Icon(
                          OcIcons.dns,
                          size: 14,
                          color: colors.textSecondary,
                        ),
                        const SizedBox(width: OcSpace.s6),
                        Text(
                          domain,
                          style: OcText.mono.copyWith(
                            color: colors.text,
                            fontSize: 12.5,
                          ),
                        ),
                      ],
                    ),
                  ),
                const SizedBox(height: OcSpace.s12),
              ],

              // Config fields section
              if (plugin.configFields.isNotEmpty) ...[
                const SectionLabel('Configuration'),
                const SizedBox(height: OcSpace.s6),
                PluginConfigFields(
                  serverKey: widget.serverKey,
                  fields: plugin.configFields,
                  values: _configValues,
                  onChanged: (key, val) =>
                      setState(() => _configValues[key] = val),
                ),
                const SizedBox(height: OcSpace.s12),
              ],

              // Licence section
              if (rev.licenseNeedsAcceptance) ...[
                Hoverable(
                  onTap: () =>
                      setState(() => _licenseAccepted = !_licenseAccepted),
                  builder: (context, state) => Row(
                    children: [
                      Icon(
                        _licenseAccepted
                            ? OcIcons.checkBox
                            : OcIcons.checkBoxOutlineBlank,
                        size: 18,
                        color: _licenseAccepted
                            ? colors.text
                            : colors.textMuted,
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Text(
                          'I accept the ${plugin.license.isNotEmpty ? plugin.license : "plugin's"} licence terms',
                          style: OcText.body.copyWith(color: colors.text),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: OcSpace.s12),
              ],

              if (_error != null) ...[
                InlineError(_error!),
                const SizedBox(height: OcSpace.s12),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
