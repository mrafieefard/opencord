import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/upload_rules.dart';
import 'package:opencord/features/settings/server/storage_page.dart';
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

/// Server settings → Upload settings (§8.2): blocked file extensions, and
/// for the owner where files are stored.
class ServerUploadsPage extends ConsumerStatefulWidget {
  const ServerUploadsPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerUploadsPage> createState() => _ServerUploadsPageState();
}

class _ServerUploadsPageState extends ConsumerState<ServerUploadsPage>
    with SaveBarChanges {
  /// As saved; kept here for the save bar (see [SaveBarChanges]).
  late List<String> _saved = _read();
  late List<String> _blocked = List.of(_saved);
  final _addController = TextEditingController();
  String? _error;

  List<String> _read() =>
      (ref.read(serverProvider(widget.serverKey)).data?.uploadSettings ??
              UploadSettings.fallback)
          .blockedExtensions;

  @override
  void dispose() {
    _addController.dispose();
    super.dispose();
  }

  @override
  bool get unsavedChanges => !listEquals(_blocked, _saved);

  @override
  Future<void> saveChanges() async {
    final blocked = List.of(_blocked);
    await ref
        .read(repositoryProvider)
        .updateUploadSettings(widget.serverKey, blockedExtensions: blocked);
    _saved = blocked;
  }

  @override
  void discardChanges() {
    if (!mounted) return;
    setState(() {
      _blocked = List.of(_saved);
      _error = null;
    });
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
    changesEdited();
  }

  void _removeExtension(String ext) {
    setState(() {
      _blocked.remove(ext);
    });
    changesEdited();
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
    changesEdited();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final saved = ref.watch(
      serverProvider(
        widget.serverKey,
      ).select((s) => s.data?.uploadSettings.blockedExtensions),
    );
    if (saved != null && !listEquals(saved, _saved)) {
      // Saved here or elsewhere: what is not being edited follows it.
      if (!unsavedChanges) _blocked = List.of(saved);
      _saved = saved;
    }
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
        if (ref.watch(
              serverProvider(widget.serverKey).select((s) => s.data?.isOwner),
            ) ??
            false) ...[
          const SizedBox(height: OcSpace.s32),
          ServerStoragePage(serverKey: widget.serverKey),
        ],
      ],
    );
  }
}
