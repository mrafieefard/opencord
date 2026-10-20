import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/storage_dialogs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Provider for server storage information (§8.2, owner only).
class StorageNotifier extends AsyncNotifier<StorageInfo> {
  StorageNotifier(this.serverKey);

  final String serverKey;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  @override
  Future<StorageInfo> build() {
    final sub = _repository.events.listen((event) {
      if (event is StorageEvent && event.serverKey == serverKey) {
        state = AsyncData(event.storage);
      }
    });
    ref.onDispose(sub.cancel);
    return _repository.storage(serverKey);
  }

  void set(StorageInfo info) => state = AsyncData(info);

  Future<void> retry() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(() => _repository.storage(serverKey));
  }
}

final storageProvider = AsyncNotifierProvider.autoDispose
    .family<StorageNotifier, StorageInfo, String>(StorageNotifier.new);

/// Where files are stored (§8.2), the owner's part of Upload settings:
/// backends, moving files, room for uploads in progress.
class ServerStoragePage extends ConsumerWidget {
  const ServerStoragePage({super.key, required this.serverKey});

  final String serverKey;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final storageState = ref.watch(storageProvider(serverKey));

    return switch (storageState) {
      AsyncData(:final value) => _StorageContent(
        serverKey: serverKey,
        storage: value,
      ),
      AsyncError(:final error) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InlineError(
            error is RepoException
                ? error.message
                : 'Storage settings could not be loaded.',
          ),
          const SizedBox(height: OcSpace.s12),
          OcButton(
            label: 'Try again',
            onPressed: () =>
                ref.read(storageProvider(serverKey).notifier).retry(),
          ),
        ],
      ),
      _ => const Padding(
        padding: EdgeInsets.all(OcSpace.s24),
        child: Center(child: OcSpinner(size: 20)),
      ),
    };
  }
}

class _StorageContent extends ConsumerStatefulWidget {
  const _StorageContent({required this.serverKey, required this.storage});

  final String serverKey;
  final StorageInfo storage;

  @override
  ConsumerState<_StorageContent> createState() => _StorageContentState();
}

class _StorageContentState extends ConsumerState<_StorageContent> {
  OpencordRepository get _repository => ref.read(repositoryProvider);

  Future<void> _activate(StorageBackend backend) async {
    try {
      final updated = await _repository.activateStorage(
        widget.serverKey,
        backend.id,
      );
      if (!mounted) return;
      ref.read(storageProvider(widget.serverKey).notifier).set(updated);
      showOcToast(context, 'Activated ${backend.name}');
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    }
  }

  Future<void> _test(StorageBackend backend) async {
    try {
      final result = await _repository.testStorage(
        widget.serverKey,
        storageId: backend.id,
      );
      if (mounted) {
        showOcToast(
          context,
          result.ok
              ? 'Test passed for ${backend.name}'
              : 'Test failed for ${backend.name}',
        );
      }
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    }
  }

  Future<void> _remove(StorageBackend backend) async {
    final confirmed = await confirmAction(
      context,
      title: 'Remove ${backend.name}?',
      message:
          'This storage backend will be deleted from the server configuration.',
      action: 'Remove',
    );
    if (!confirmed) return;
    try {
      final updated = await _repository.removeStorage(
        widget.serverKey,
        backend.id,
      );
      if (!mounted) return;
      ref.read(storageProvider(widget.serverKey).notifier).set(updated);
      showOcToast(context, 'Removed ${backend.name}');
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    }
  }

  Future<void> _startMove() async {
    try {
      final updated = await _repository.startStorageMove(widget.serverKey);
      if (!mounted) return;
      ref.read(storageProvider(widget.serverKey).notifier).set(updated);
      showOcToast(context, 'Moving files to active storage');
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    }
  }

  Future<void> _stopMove() async {
    try {
      final updated = await _repository.stopStorageMove(widget.serverKey);
      if (!mounted) return;
      ref.read(storageProvider(widget.serverKey).notifier).set(updated);
      showOcToast(context, 'Stopped moving files');
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    }
  }

  /// What can be done to [backend]; empty for the server's own storage
  /// while it is active, which then shows no menu.
  List<OcMenuItem> _backendActions(StorageBackend backend) {
    final isLocal = backend.kind == StorageKind.local;
    final isS3 = backend.kind == StorageKind.s3;
    return [
      if (!backend.active)
        OcMenuItem(
          label: 'Activate',
          icon: OcIcons.check,
          onSelected: () => _activate(backend),
        ),
      if (isS3) ...[
        OcMenuItem(
          label: 'Test',
          icon: OcIcons.sync,
          onSelected: () => _test(backend),
        ),
        OcMenuItem(
          label: 'Edit',
          icon: OcIcons.edit,
          onSelected: () => showOcDialog(
            context: context,
            builder: (_) =>
                S3Dialog(serverKey: widget.serverKey, existing: backend),
          ),
        ),
      ],
      if (!isLocal && !backend.active && backend.fileCount == 0)
        OcMenuItem(
          label: 'Remove',
          icon: OcIcons.delete,
          onSelected: () => _remove(backend),
        ),
    ];
  }

  void _showBackendMenu(BuildContext btnContext, List<OcMenuItem> actions) {
    final rect = globalRectOf(btnContext);
    showOcMenu(
      context: context,
      position: rect.bottomLeft.translate(0, 4),
      entries: actions,
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final storage = widget.storage;
    final move = storage.moveStatus;
    final hasFilesElsewhere = storage.backends.any(
      (b) => !b.active && b.fileCount > 0,
    );
    final showMoveSection = hasFilesElsewhere || (move != null && move.running);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                'Storage',
                style: OcText.title.copyWith(color: colors.text),
              ),
            ),
            OcButton(
              label: 'Connect a bucket',
              icon: OcIcons.add,
              onPressed: () => showOcDialog(
                context: context,
                builder: (_) => S3Dialog(serverKey: widget.serverKey),
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        SettingsSection(
          children: [
            for (final backend in storage.backends)
              Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s16,
                  vertical: OcSpace.s12,
                ),
                child: Row(
                  children: [
                    Icon(
                      backend.kind == StorageKind.local
                          ? OcIcons.storage
                          : OcIcons.cloudUpload,
                      size: OcSize.iconRow,
                      color: colors.textSecondary,
                    ),
                    const SizedBox(width: OcSpace.s12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Text(
                                backend.name,
                                style: OcText.body.copyWith(
                                  fontWeight: FontWeight.w600,
                                  color: colors.text,
                                ),
                              ),
                              const SizedBox(width: OcSpace.s8),
                              Text(
                                backend.kind == StorageKind.local
                                    ? 'Local'
                                    : 'S3',
                                style: OcText.small.copyWith(
                                  color: colors.textMuted,
                                ),
                              ),
                              if (backend.active) ...[
                                const SizedBox(width: OcSpace.s8),
                                Container(
                                  padding: const EdgeInsets.symmetric(
                                    horizontal: 8,
                                    vertical: 2,
                                  ),
                                  decoration: BoxDecoration(
                                    color: colors.hover,
                                    borderRadius: BorderRadius.circular(
                                      OcRadius.searchPill,
                                    ),
                                    border: Border.all(color: colors.border),
                                  ),
                                  child: Text(
                                    'Active',
                                    style: OcText.small.copyWith(
                                      fontWeight: FontWeight.w600,
                                      color: colors.text,
                                    ),
                                  ),
                                ),
                              ],
                            ],
                          ),
                          const SizedBox(height: 2),
                          Text(
                            '${backend.fileCount} files · ${formatFileSize(backend.bytes)}',
                            style: OcText.small.copyWith(
                              color: colors.textSecondary,
                            ),
                          ),
                          if (backend.lastError case final err?) ...[
                            const SizedBox(height: 2),
                            Text(
                              err,
                              style: OcText.small.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    if (_backendActions(backend) case final actions
                        when actions.isNotEmpty)
                      Builder(
                        builder: (btnContext) => OcIconButton(
                          icon: OcIcons.moreVert,
                          tooltip: 'Actions for ${backend.name}',
                          onPressed: () =>
                              _showBackendMenu(btnContext, actions),
                        ),
                      ),
                  ],
                ),
              ),
          ],
        ),
        if (showMoveSection) ...[
          const SizedBox(height: OcSpace.s24),
          SettingsSection(
            title: 'Move files',
            footer:
                'Moves all files stored on other backends to the active storage.',
            children: [
              Padding(
                padding: const EdgeInsets.all(OcSpace.s16),
                child: move != null && move.running
                    ? Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Row(
                            children: [
                              Expanded(
                                child: Text(
                                  '${move.filesDone} / ${move.filesTotal} files · ${formatFileSize(move.bytesDone)} / ${formatFileSize(move.bytesTotal)}',
                                  style: OcText.body.copyWith(
                                    color: colors.text,
                                  ),
                                ),
                              ),
                              OcButton(
                                label: 'Stop',
                                icon: OcIcons.stop,
                                onPressed: _stopMove,
                              ),
                            ],
                          ),
                          const SizedBox(height: OcSpace.s8),
                          ClipRRect(
                            borderRadius: BorderRadius.circular(2),
                            child: LinearProgressIndicator(
                              value: move.filesTotal > 0
                                  ? move.filesDone / move.filesTotal
                                  : null,
                              backgroundColor: colors.border,
                              color: colors.text,
                              minHeight: 4,
                            ),
                          ),
                          if (move.failed > 0) ...[
                            const SizedBox(height: 4),
                            Text(
                              '${move.failed} files failed to move',
                              style: OcText.small.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                          ],
                        ],
                      )
                    : Row(
                        children: [
                          Expanded(
                            child: Text(
                              'Files are currently stored on multiple backends.',
                              style: OcText.body.copyWith(color: colors.text),
                            ),
                          ),
                          OcButton(
                            label: 'Move existing files here',
                            icon: OcIcons.sync,
                            onPressed: _startMove,
                          ),
                        ],
                      ),
              ),
            ],
          ),
        ],
        const SizedBox(height: OcSpace.s24),
        Padding(
          padding: const EdgeInsets.only(left: 4),
          child: Text(
            'Room for uploads in progress: ${formatFileSize(storage.tempFreeBytes)}',
            style: OcText.small.copyWith(color: colors.textMuted),
          ),
        ),
      ],
    );
  }
}
