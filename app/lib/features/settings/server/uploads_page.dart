import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/upload_rules.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Server settings → Uploads (§8.2): blocked file extensions and voice message maximum.
class ServerUploadsPage extends ConsumerStatefulWidget {
  const ServerUploadsPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerUploadsPage> createState() => _ServerUploadsPageState();
}

class _ServerUploadsPageState extends ConsumerState<ServerUploadsPage> {
  Map<Object, bool Function()>? _unsaved;

  late List<String> _blocked;
  late int _voiceSeconds;
  late final TextEditingController _addController;
  var _saving = false;
  String? _error;

  UploadSettings? get _settings =>
      ref.read(serverProvider(widget.serverKey)).data?.uploadSettings;

  @override
  void initState() {
    super.initState();
    final s = _settings ?? UploadSettings.fallback;
    _blocked = List.of(s.blockedExtensions);
    _voiceSeconds = s.voiceMessageMaxSeconds;
    _addController = TextEditingController();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved = SettingsScope.of(context)?..[this] = () => _changed;
  }

  @override
  void dispose() {
    _unsaved?.remove(this);
    _addController.dispose();
    super.dispose();
  }

  bool get _changed {
    final s = _settings;
    if (s == null) return false;
    if (_voiceSeconds != s.voiceMessageMaxSeconds) return true;
    if (_blocked.length != s.blockedExtensions.length) return true;
    for (var i = 0; i < _blocked.length; i++) {
      if (_blocked[i] != s.blockedExtensions[i]) return true;
    }
    return false;
  }

  void _addExtension(String input) {
    final norm = normalizeExtension(input);
    if (norm == null) {
      setState(
        () => _error =
            'Enter a valid extension (1 to 20 letters, numbers, or dots)',
      );
      return;
    }
    setState(() {
      _error = null;
      if (!_blocked.contains(norm)) {
        _blocked.add(norm);
        _blocked.sort();
      }
      _addController.clear();
    });
  }

  void _removeExtension(String ext) {
    setState(() {
      _blocked.remove(ext);
    });
  }

  void _blockPrograms() {
    setState(() {
      for (final ext in blockProgramsPreset) {
        if (!_blocked.contains(ext)) {
          _blocked.add(ext);
        }
      }
      _blocked.sort();
    });
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref
          .read(repositoryProvider)
          .updateUploadSettings(
            widget.serverKey,
            blockedExtensions: _blocked,
            voiceMessageMaxSeconds: _voiceSeconds,
          );
      if (mounted) showOcToast(context, 'Upload settings saved');
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Blocked file types',
          footer:
              'Files with these extensions will be rejected when members try to upload them.',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: OcTextField(
                          controller: _addController,
                          semanticLabel: 'Add extension',
                          hint: 'Add extension, e.g. exe or sh',
                          onSubmitted: _addExtension,
                        ),
                      ),
                      const SizedBox(width: OcSpace.s8),
                      OcButton(
                        label: 'Block programs',
                        icon: OcIcons.block,
                        onPressed: _blockPrograms,
                      ),
                    ],
                  ),
                  if (_error case final error?) ...[
                    const SizedBox(height: OcSpace.s8),
                    InlineError(error),
                  ],
                  if (_blocked.isNotEmpty) ...[
                    const SizedBox(height: OcSpace.s16),
                    Wrap(
                      spacing: OcSpace.s8,
                      runSpacing: OcSpace.s8,
                      children: [
                        for (final ext in _blocked)
                          Container(
                            padding: const EdgeInsets.only(
                              left: 10,
                              right: 4,
                              top: 4,
                              bottom: 4,
                            ),
                            decoration: BoxDecoration(
                              color: colors.hover,
                              borderRadius: BorderRadius.circular(
                                OcRadius.searchPill,
                              ),
                              border: Border.all(color: colors.border),
                            ),
                            child: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text(
                                  ext,
                                  style: OcText.small.copyWith(
                                    color: colors.text,
                                  ),
                                ),
                                const SizedBox(width: 4),
                                OcIconButton(
                                  icon: OcIcons.close,
                                  size: OcIconButtonSize.compact,
                                  tooltip: 'Remove $ext',
                                  onPressed: () => _removeExtension(ext),
                                ),
                              ],
                            ),
                          ),
                      ],
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          title: 'Voice messages',
          children: [
            SettingsSliderRow(
              title: 'Maximum length',
              value: _voiceSeconds,
              min: 30,
              max: 1800,
              step: 30,
              shown:
                  '${_voiceSeconds ~/ 60}:${(_voiceSeconds % 60).toString().padLeft(2, '0')}',
              onChanged: (val) => setState(() => _voiceSeconds = val),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s24),
        Align(
          alignment: Alignment.centerLeft,
          child: OcButton.primary(
            label: 'Save changes',
            busy: _saving,
            onPressed: _changed ? _save : null,
          ),
        ),
      ],
    );
  }
}
