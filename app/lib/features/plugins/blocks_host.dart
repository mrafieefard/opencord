import 'dart:async';
import 'dart:io';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/markdown.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/plugins/plugin_icons.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord_blocks/opencord_blocks.dart';
import 'package:url_launcher/url_launcher.dart';

/// Implements [BlocksHost] for Opencord Desktop.
class OpencordBlocksHost implements BlocksHost {
  OpencordBlocksHost({
    required this.context,
    required this.ref,
    required this.serverKey,
    required this.pluginId,
    required this.viewId,
  });

  final BuildContext context;
  final WidgetRef ref;
  final String serverKey;
  final String pluginId;
  final ValueListenable<String?> viewId;

  @override
  IconData? icon(String name) => pluginIcons[name];

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async {
    final repo = ref.read(repositoryProvider);
    if (source.containsKey('attachment')) {
      final id = source['attachment'];
      final attachmentId = id is int ? id : int.tryParse(id.toString());
      if (attachmentId == null) return null;
      final bytes = await repo.attachmentBytes(serverKey, attachmentId);
      return MemoryImage(bytes);
    }
    final file = await repo.pluginImageFile(serverKey, pluginId, source);
    return file != null ? FileImage(File(file)) : null;
  }

  @override
  Widget user(String id, double size, bool showName) {
    final userId = int.tryParse(id);
    final serverData = ref.watch(serverProvider(serverKey)).data;
    final name = serverData?.authorOf(userId ?? 0).name ?? 'User $id';
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        OcAvatar(id: userId ?? id, name: name, size: size),
        if (showName) ...[
          const SizedBox(width: 8),
          Text(name, style: OcText.body),
        ],
      ],
    );
  }

  @override
  Widget richText(String markdown, TextStyle style) {
    final serverData = ref.watch(serverProvider(serverKey)).data;
    final blocks = parseMarkdown(markdown);
    return MarkdownView(
      blocks: blocks,
      links: MarkdownContext(
        userName: (id) => serverData?.authorOf(id).name,
        channelName: (id) => serverData?.channels[id]?.name,
      ),
      color: style.color ?? context.oc.text,
    );
  }

  @override
  Future<String?> pickFile({
    required bool image,
    List<String> accept = const [],
  }) async {
    final typeGroup = XTypeGroup(
      label: image ? 'images' : 'files',
      extensions: accept.isNotEmpty ? accept : null,
    );
    final file = await openFile(acceptedTypeGroups: [typeGroup]);
    return file?.path;
  }

  @override
  Future<bool> confirmLink(String url) async {
    final confirmed = await showOcDialog<bool>(
      context: context,
      builder: (dialogContext) => OcDialog(
        title: 'Open this link?',
        actions: [
          OcButton(
            label: 'Cancel',
            onPressed: () => Navigator.pop(dialogContext, false),
          ),
          OcButton.primary(
            label: 'Open',
            autofocus: true,
            onPressed: () => Navigator.pop(dialogContext, true),
          ),
        ],
        child: SelectableText(url, style: OcText.body),
      ),
    );
    if (confirmed == true) {
      final uri = Uri.tryParse(url);
      if (uri != null) {
        await launchUrl(uri, mode: LaunchMode.externalApplication);
      }
      return true;
    }
    return false;
  }

  @override
  void copy(String text) async {
    try {
      await Clipboard.setData(ClipboardData(text: text));
      if (context.mounted) {
        showOcToast(context, 'Copied');
      }
    } on Object catch (error) {
      appLog('copy to clipboard failed: $error', name: 'views');
    }
  }

  @override
  void openChannel(String id) {
    final channelId = int.tryParse(id);
    if (channelId != null) {
      openChannelWithToast(context, ref, serverKey, channelId);
    }
  }

  @override
  Future<void> playSound(
    String asset,
    double volume,
    VoidCallback onEnd,
  ) async {
    final repo = ref.read(repositoryProvider);
    int? draftId;
    try {
      final file = await repo.pluginAssetFile(serverKey, pluginId, asset, '');
      final draft = await repo.openSoundDraft(file);
      draftId = draft.id;
      await repo.previewSoundDraft(
        SoundWindow(
          draftId: draft.id,
          start: Duration.zero,
          length: draft.duration,
          volume: (volume.clamp(0.0, 1.0) * 100).round(),
        ),
      );
      Future.delayed(draft.duration, () {
        try {
          repo.closeSoundDraft(draft.id);
        } on Object catch (err) {
          appLog('failed to close sound draft: $err', name: 'views');
        }
        onEnd();
      });
    } on Object catch (error) {
      if (draftId != null) {
        try {
          repo.closeSoundDraft(draftId);
        } on Object catch (err) {
          appLog('failed to close sound draft: $err', name: 'views');
        }
      }
      appLog('plugin playSound failed: $error', name: 'views');
      onEnd();
    }
  }

  @override
  Future<ByteData?> font(String file) async {
    final repo = ref.read(repositoryProvider);
    final filePath = await repo.pluginAssetFile(
      serverKey,
      pluginId,
      'assets/fonts/$file',
      '',
    );
    final bytes = await File(filePath).readAsBytes();
    return ByteData.sublistView(bytes);
  }

  @override
  Future<void> upload(int nodeId, String path) async {
    var id = viewId.value;
    if (id == null) {
      final completer = Completer<String>();
      void listener() {
        if (viewId.value != null && !completer.isCompleted) {
          completer.complete(viewId.value!);
        }
      }

      viewId.addListener(listener);
      try {
        id = await completer.future.timeout(const Duration(seconds: 10));
      } finally {
        viewId.removeListener(listener);
      }
    }
    await ref.read(repositoryProvider).viewUpload(id, nodeId, path);
  }
}
