import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';

enum S3Provider {
  aws('Amazon S3'),
  r2('Cloudflare R2'),
  b2('Backblaze B2'),
  wasabi('Wasabi'),
  custom('MinIO / Custom');

  const S3Provider(this.label);
  final String label;
}

/// Dialog for connecting or editing an S3-compatible bucket (§8.2).
class S3Dialog extends ConsumerStatefulWidget {
  const S3Dialog({super.key, required this.serverKey, this.existing});

  final String serverKey;
  final StorageBackend? existing;

  @override
  ConsumerState<S3Dialog> createState() => _S3DialogState();
}

class _S3DialogState extends ConsumerState<S3Dialog> {
  late final TextEditingController _name;
  late final TextEditingController _endpoint;
  late final TextEditingController _region;
  late final TextEditingController _bucket;
  late final TextEditingController _keyPrefix;
  late final TextEditingController _accessKeyId;
  late final TextEditingController _secretAccessKey;
  late final TextEditingController _r2AccountId;

  S3Provider _provider = S3Provider.custom;
  bool _pathStyle = false;
  bool _allowHttp = false;

  bool _testing = false;
  bool _saving = false;
  StorageTestResult? _testResult;
  bool _dirtySinceTest = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    final s3 = widget.existing?.s3;
    // The name isn't tested; it only decides whether Save may be pressed.
    _name = TextEditingController(text: widget.existing?.name ?? '')
      ..addListener(() => setState(() {}));
    _endpoint = TextEditingController(text: s3?.endpoint ?? '')
      ..addListener(_onFieldChanged);
    _region = TextEditingController(text: s3?.region ?? '')
      ..addListener(_onFieldChanged);
    _bucket = TextEditingController(text: s3?.bucket ?? '')
      ..addListener(_onFieldChanged);
    _keyPrefix = TextEditingController(text: s3?.keyPrefix ?? '')
      ..addListener(_onFieldChanged);
    // Answers carry the access key masked and no secret: both start empty,
    // and are sent only when typed again.
    _accessKeyId = TextEditingController()..addListener(_onFieldChanged);
    _secretAccessKey = TextEditingController()..addListener(_onFieldChanged);
    _r2AccountId = TextEditingController()..addListener(_onR2AccountChanged);

    _pathStyle = s3?.pathStyle ?? false;
    _allowHttp = s3?.allowHttp ?? false;
  }

  @override
  void dispose() {
    _name.dispose();
    _endpoint.dispose();
    _region.dispose();
    _bucket.dispose();
    _keyPrefix.dispose();
    _accessKeyId.dispose();
    _secretAccessKey.dispose();
    _r2AccountId.dispose();
    super.dispose();
  }

  void _onFieldChanged() => setState(() {
    if (_testResult != null) _dirtySinceTest = true;
  });

  /// Why the keys as typed can't be used; null when they can. Adding needs
  /// both; editing keeps the saved ones left empty, but a new access key
  /// needs its secret.
  String? get _keysProblem {
    final key = _accessKeyId.text.trim().isNotEmpty;
    final secret = _secretAccessKey.text.trim().isNotEmpty;
    if (widget.existing == null) {
      return key && secret ? null : 'Enter the access key and its secret key.';
    }
    return key && !secret ? 'A new access key needs its secret key.' : null;
  }

  /// Whether the bucket settings differ from the saved ones; always true
  /// when adding. Unchanged, an edit is a rename and needs no test.
  bool get _bucketChanged {
    final saved = widget.existing?.s3;
    if (saved == null) return true;
    return _endpoint.text.trim() != saved.endpoint ||
        _region.text.trim() != saved.region ||
        _bucket.text.trim() != saved.bucket ||
        _keyPrefix.text.trim() != saved.keyPrefix ||
        _pathStyle != saved.pathStyle ||
        _allowHttp != saved.allowHttp ||
        _accessKeyId.text.trim().isNotEmpty ||
        _secretAccessKey.text.trim().isNotEmpty;
  }

  void _onR2AccountChanged() {
    final account = _r2AccountId.text.trim();
    if (_provider == S3Provider.r2 && account.isNotEmpty) {
      _endpoint.text = 'https://$account.r2.cloudflarestorage.com';
    }
    _onFieldChanged();
  }

  void _applyPreset(S3Provider provider) {
    setState(() {
      _provider = provider;
      _dirtySinceTest = true;
      switch (provider) {
        case S3Provider.aws:
          final reg = _region.text.trim().isEmpty
              ? 'us-east-1'
              : _region.text.trim();
          _region.text = reg;
          _endpoint.text = 'https://s3.$reg.amazonaws.com';
          _pathStyle = false;
        case S3Provider.r2:
          _region.text = 'auto';
          final acc = _r2AccountId.text.trim();
          _endpoint.text = acc.isNotEmpty
              ? 'https://$acc.r2.cloudflarestorage.com'
              : 'https://{account}.r2.cloudflarestorage.com';
          _pathStyle = true;
        case S3Provider.b2:
          final reg = _region.text.trim().isEmpty
              ? 'us-west-004'
              : _region.text.trim();
          _region.text = reg;
          _endpoint.text = 'https://s3.$reg.backblazeb2.com';
          _pathStyle = false;
        case S3Provider.wasabi:
          final reg = _region.text.trim().isEmpty
              ? 'eu-central-1'
              : _region.text.trim();
          _region.text = reg;
          _endpoint.text = 'https://s3.$reg.wasabisys.com';
          _pathStyle = false;
        case S3Provider.custom:
          _pathStyle = true;
      }
    });
  }

  S3Config _buildConfig() {
    return S3Config(
      endpoint: _endpoint.text.trim(),
      region: _region.text.trim(),
      bucket: _bucket.text.trim(),
      keyPrefix: _keyPrefix.text.trim(),
      pathStyle: _pathStyle,
      allowHttp: _allowHttp,
      accessKeyId: _accessKeyId.text.trim(),
      secretAccessKey: _secretAccessKey.text.trim(),
    );
  }

  Future<void> _test() async {
    setState(() {
      _testing = true;
      _error = null;
    });
    try {
      // Unchanged settings test the saved bucket (with its saved secret);
      // anything else tests exactly what Save would send.
      final result = _bucketChanged
          ? await ref
                .read(repositoryProvider)
                .testStorage(
                  widget.serverKey,
                  s3: _buildConfig(),
                  keysFrom: widget.existing?.id,
                )
          : await ref
                .read(repositoryProvider)
                .testStorage(widget.serverKey, storageId: widget.existing?.id);
      if (mounted) {
        setState(() {
          _testResult = result;
          _dirtySinceTest = false;
        });
      }
    } on RepoException catch (e) {
      if (mounted) {
        setState(() {
          _error = e.message;
        });
      }
    } finally {
      if (mounted) {
        setState(() {
          _testing = false;
        });
      }
    }
  }

  Future<void> _save() async {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Storage name cannot be empty');
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final repository = ref.read(repositoryProvider);
      if (widget.existing case final existing?) {
        await repository.updateStorage(
          widget.serverKey,
          existing.id,
          name: name,
          s3: _bucketChanged ? _buildConfig() : null,
        );
      } else {
        await repository.addStorage(widget.serverKey, name, _buildConfig());
      }
      if (mounted) {
        Navigator.of(context).pop(true);
      }
    } on RepoException catch (e) {
      if (mounted) {
        setState(() {
          _error = e.message;
          _saving = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final isEdit = widget.existing != null;
    final bucketChanged = _bucketChanged;
    final keysProblem = _keysProblem;
    final tested =
        keysProblem == null && _testResult?.ok == true && !_dirtySinceTest;
    final canSave =
        _name.text.trim().isNotEmpty &&
        (!bucketChanged || tested) &&
        !_testing &&
        !_saving;

    return OcDialog(
      title: isEdit ? 'Edit bucket' : 'Connect a bucket',
      width: 520,
      showClose: true,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.of(context).pop()),
        OcButton.primary(
          label: 'Save',
          busy: _saving,
          onPressed: canSave ? _save : null,
        ),
      ],
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            // Provider Preset Buttons
            ChoiceChips<S3Provider>(
              options: [for (final p in S3Provider.values) (p, p.label)],
              value: _provider,
              onChanged: _applyPreset,
            ),
            const SizedBox(height: OcSpace.s16),
            if (_provider == S3Provider.r2) ...[
              OcTextField(
                controller: _r2AccountId,
                semanticLabel: 'Account ID',
                hint: 'Cloudflare Account ID',
              ),
              const SizedBox(height: OcSpace.s12),
            ],
            OcTextField(
              controller: _name,
              semanticLabel: 'Storage name',
              hint: 'Storage name',
            ),
            const SizedBox(height: OcSpace.s12),
            OcTextField(
              controller: _endpoint,
              semanticLabel: 'Endpoint',
              hint: 'Endpoint',
            ),
            const SizedBox(height: OcSpace.s12),
            Row(
              children: [
                Expanded(
                  child: OcTextField(
                    controller: _region,
                    semanticLabel: 'Region',
                    hint: 'Region',
                  ),
                ),
                const SizedBox(width: OcSpace.s12),
                Expanded(
                  child: OcTextField(
                    controller: _bucket,
                    semanticLabel: 'Bucket',
                    hint: 'Bucket',
                  ),
                ),
              ],
            ),
            const SizedBox(height: OcSpace.s12),
            OcTextField(
              controller: _keyPrefix,
              semanticLabel: 'Key prefix',
              hint: 'Key prefix (optional)',
            ),
            const SizedBox(height: OcSpace.s12),
            OcTextField(
              controller: _accessKeyId,
              semanticLabel: 'Access key ID',
              hint: switch (widget.existing?.s3?.accessKeyId) {
                final saved? => 'Access key ID (saved: $saved)',
                null => 'Access key ID',
              },
            ),
            const SizedBox(height: OcSpace.s12),
            OcTextField(
              controller: _secretAccessKey,
              semanticLabel: 'Secret access key',
              hint: 'Secret access key',
              obscure: true,
            ),
            if (isEdit && keysProblem != null) ...[
              const SizedBox(height: 4),
              Text(
                keysProblem,
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ] else if (isEdit) ...[
              const SizedBox(height: 4),
              Text(
                'Leave both keys empty to keep the saved ones.',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ],
            const SizedBox(height: OcSpace.s12),
            Row(
              children: [
                OcSwitch(
                  value: _pathStyle,
                  onChanged: (val) {
                    setState(() {
                      _pathStyle = val;
                      _dirtySinceTest = true;
                    });
                  },
                ),
                const SizedBox(width: OcSpace.s8),
                Text(
                  'Use path-style access',
                  style: OcText.body.copyWith(color: colors.text),
                ),
              ],
            ),
            const SizedBox(height: OcSpace.s8),
            Row(
              children: [
                OcSwitch(
                  value: _allowHttp,
                  onChanged: (val) {
                    setState(() {
                      _allowHttp = val;
                      _dirtySinceTest = true;
                    });
                  },
                ),
                const SizedBox(width: OcSpace.s8),
                Text(
                  'Allow plain HTTP',
                  style: OcText.body.copyWith(color: colors.text),
                ),
              ],
            ),
            if (_allowHttp) ...[
              const SizedBox(height: 4),
              Text(
                'Plain HTTP is unencrypted and transmits credentials and data in clear text.',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ],
            const SizedBox(height: OcSpace.s16),
            Row(
              children: [
                OcButton(
                  label: 'Test',
                  icon: OcIcons.sync,
                  busy: _testing,
                  onPressed: _test,
                ),
                const SizedBox(width: OcSpace.s12),
                if (_testResult != null)
                  Expanded(
                    child: Text(
                      _dirtySinceTest
                          ? 'Settings changed since test'
                          : _testResult!.ok
                          ? 'All test steps passed'
                          : 'Test failed',
                      style: OcText.small.copyWith(
                        color: _dirtySinceTest
                            ? colors.textSecondary
                            : _testResult!.ok
                            ? colors.text
                            : colors.textMuted,
                      ),
                    ),
                  ),
              ],
            ),
            if (_testResult case final result?) ...[
              const SizedBox(height: OcSpace.s12),
              Container(
                padding: const EdgeInsets.all(OcSpace.s12),
                decoration: BoxDecoration(
                  color: colors.hover,
                  borderRadius: BorderRadius.circular(OcRadius.section),
                  border: Border.all(color: colors.border),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    for (final step in result.steps)
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 2),
                        child: Row(
                          children: [
                            Icon(
                              step.ok ? OcIcons.check : OcIcons.close,
                              size: 16,
                              color: colors.text,
                            ),
                            const SizedBox(width: OcSpace.s8),
                            Expanded(
                              child: Text(
                                '${step.name}: ${step.detail}',
                                style: OcText.small.copyWith(
                                  color: colors.text,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ],
            if (_error case final err?) ...[
              const SizedBox(height: OcSpace.s12),
              InlineError(err),
            ],
          ],
        ),
      ),
    );
  }
}
