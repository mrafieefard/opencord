import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/upload_rules.dart';

export 'package:opencord/core/providers/providers.dart' show ChannelRef;

const draftsKey = 'ui.drafts';

/// How long typing settles before the draft is written to disk.
const draftSaveDelay = Duration(milliseconds: 400);

String _draftKey(ChannelRef channel) => '${channel.server}#${channel.channel}';

/// What one channel's composer holds (§4.6): the text, and the message it
/// replies to or edits.
@immutable
class ComposerState {
  const ComposerState({
    this.draft = '',
    this.replyTo,
    this.editing,
    this.stashedDraft,
    this.staged = const [],
  });

  final String draft;
  final Message? replyTo;
  final Message? editing;

  /// The draft put aside while editing, given back afterwards.
  final String? stashedDraft;

  /// Files staged to be attached to the message.
  final List<StagedFile> staged;

  bool get hasProblem => staged.any((f) => f.problem != null);

  @override
  bool operator ==(Object other) =>
      other is ComposerState &&
      other.draft == draft &&
      other.replyTo == replyTo &&
      other.editing == editing &&
      other.stashedDraft == stashedDraft &&
      listEquals(other.staged, staged);

  @override
  int get hashCode => Object.hash(
    draft,
    replyTo,
    editing,
    stashedDraft,
    Object.hashAll(staged),
  );
}

/// One channel's composer. Drafts survive switching channels and
/// restarting (§4.6, §16); replies and edits last for this run.
class ComposerNotifier extends Notifier<ComposerState> {
  ComposerNotifier(this.channel);

  final ChannelRef channel;
  Timer? _save;
  String _pending = '';

  /// Read once: the pending save may run while the provider is disposed,
  /// when `ref` can no longer be used.
  late KeyValueStore _store;

  Map<String, String> _drafts() {
    final saved = _store.read(draftsKey);
    if (saved == null) return {};
    try {
      final json = jsonDecode(saved);
      return {
        if (json is Map)
          for (final MapEntry(:key, :value) in json.entries)
            if (key is String && value is String) key: value,
      };
    } on FormatException {
      return {};
    }
  }

  @override
  ComposerState build() {
    _store = ref.watch(keyValueStoreProvider);
    ref.onDispose(() {
      if (_save?.isActive ?? false) {
        _save!.cancel();
        _write(_pending);
      }
    });
    return ComposerState(draft: _drafts()[_draftKey(channel)] ?? '');
  }

  /// The text typed so far; while editing, the draft put aside.
  String get _savedText =>
      state.editing == null ? state.draft : (state.stashedDraft ?? '');

  void _write(String text) {
    final drafts = _drafts();
    if (text.trim().isEmpty) {
      drafts.remove(_draftKey(channel));
    } else {
      drafts[_draftKey(channel)] = text;
    }
    _store.write(draftsKey, drafts.isEmpty ? null : jsonEncode(drafts));
  }

  void _scheduleSave() {
    // Kept apart from `state`, which cannot be read while disposing.
    _pending = _savedText;
    _save?.cancel();
    _save = Timer(draftSaveDelay, () => _write(_pending));
  }

  void setDraft(String text) {
    if (text == state.draft) return;
    state = ComposerState(
      draft: text,
      replyTo: state.replyTo,
      editing: state.editing,
      stashedDraft: state.stashedDraft,
      staged: state.staged,
    );
    if (state.editing == null) _scheduleSave();
  }

  void reply(Message message) {
    state = ComposerState(
      draft: _savedText,
      replyTo: message,
      staged: state.staged,
    );
  }

  void edit(Message message) {
    state = ComposerState(
      draft: message.content,
      editing: message,
      stashedDraft: _savedText,
      staged: state.staged,
    );
  }

  /// Escape (§4.6): drops the reply or the edit, keeping the draft.
  void cancel() {
    state = ComposerState(draft: _savedText, staged: state.staged);
  }

  void stageFiles(List<StagedFile> files) {
    final serverData = ref.read(serverProvider(channel.server)).data;
    final limit =
        serverData?.myUploadLimit ?? UploadSettings.fallback.maxFileBytes;
    final blocked = serverData?.uploadSettings.blockedExtensions ?? const [];

    final current = List<StagedFile>.of(state.staged);
    for (final file in files) {
      final problem =
          file.problem ??
          stageProblem(
            name: file.name,
            size: file.size,
            limit: limit,
            blocked: blocked,
          );
      current.add(file.copyWith(problem: () => problem));
    }
    final updated = <StagedFile>[];
    for (var i = 0; i < current.length; i++) {
      final file = current[i];
      if (i >= 10 && file.problem == null) {
        updated.add(file.copyWith(problem: () => StageProblem.tooMany));
      } else {
        updated.add(file);
      }
    }
    state = ComposerState(
      draft: state.draft,
      replyTo: state.replyTo,
      editing: state.editing,
      stashedDraft: state.stashedDraft,
      staged: updated,
    );
  }

  void stageFile(StagedFile file) => stageFiles([file]);

  Future<void> stagePaths(List<String> paths) async {
    final files = <StagedFile>[];
    for (final path in paths) {
      final file = File(path);
      if (!await file.exists()) continue;
      final size = await file.length();
      final name = file.uri.pathSegments.lastWhere(
        (s) => s.isNotEmpty,
        orElse: () => 'file',
      );
      Uint8List? preview;
      final ext = name.split('.').last.toLowerCase();
      if (const {'png', 'jpg', 'jpeg', 'gif', 'webp'}.contains(ext) &&
          size <= 20 * 1024 * 1024) {
        try {
          preview = await file.readAsBytes();
        } catch (_) {}
      }
      final id = '${DateTime.now().microsecondsSinceEpoch}_${name.hashCode}';
      files.add(
        StagedFile(
          id: id,
          name: name,
          size: size,
          path: path,
          preview: preview,
        ),
      );
    }
    stageFiles(files);
  }

  void removeStaged(String id) {
    final remaining = state.staged.where((f) => f.id != id).toList();
    final serverData = ref.read(serverProvider(channel.server)).data;
    final limit =
        serverData?.myUploadLimit ?? UploadSettings.fallback.maxFileBytes;
    final blocked = serverData?.uploadSettings.blockedExtensions ?? const [];

    final updated = <StagedFile>[];
    for (var i = 0; i < remaining.length; i++) {
      final file = remaining[i];
      final problem = i >= 10
          ? StageProblem.tooMany
          : stageProblem(
              name: file.name,
              size: file.size,
              limit: limit,
              blocked: blocked,
            );
      updated.add(file.copyWith(problem: () => problem));
    }
    state = ComposerState(
      draft: state.draft,
      replyTo: state.replyTo,
      editing: state.editing,
      stashedDraft: state.stashedDraft,
      staged: updated,
    );
  }

  void clearStaged() {
    state = ComposerState(
      draft: state.draft,
      replyTo: state.replyTo,
      editing: state.editing,
      stashedDraft: state.stashedDraft,
      staged: const [],
    );
  }

  void sent() {
    final wasEditing = state.editing != null;
    state = wasEditing
        ? ComposerState(draft: state.stashedDraft ?? '')
        : const ComposerState();
    _scheduleSave();
  }
}

final composerProvider =
    NotifierProvider.family<ComposerNotifier, ComposerState, ChannelRef>(
      ComposerNotifier.new,
    );
