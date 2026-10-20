import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/upload_rules.dart';
import 'package:opencord/features/chat/attachments/file_icon.dart';

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
/// Images staged with a thumbnail; larger ones show their icon.
const _previewExtensions = {'png', 'jpg', 'jpeg', 'gif', 'webp'};
const _previewMaxBytes = 20 * 1024 * 1024;

class ComposerNotifier extends Notifier<ComposerState> {
  ComposerNotifier(this.channel);

  final ChannelRef channel;
  Timer? _save;
  String _pending = '';

  /// Read once: the pending save may run while the provider is disposed,
  /// when `ref` can no longer be used.
  late KeyValueStore _store;

  OpencordRepository get _repository => ref.read(repositoryProvider);

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
    // This channel's staged files' uploads; nothing else routes these.
    final uploads = ref.watch(repositoryProvider).events.listen((event) {
      if (event is StagedUploadEvent &&
          event.serverKey == channel.server &&
          event.channelId == channel.channel) {
        _applyUpload(event);
      }
    });
    ref.onDispose(uploads.cancel);
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

  /// Staged files' ids, unique in the app: the core tells their uploads
  /// apart by server and id, whatever the channel.
  static var _nextStaged = 0;

  String _stagedId() =>
      'staged-${DateTime.now().microsecondsSinceEpoch}-${_nextStaged++}';

  /// Each file's problem, worked out again for the whole list: a removal
  /// can bring a file back under the server's attachment limit.
  List<StagedFile> _checked(List<StagedFile> files) {
    final data = ref.read(serverProvider(channel.server)).data;
    final settings = data?.uploadSettings ?? UploadSettings.fallback;
    final limit = data?.myUploadLimit ?? settings.maxFileBytes;
    return [
      for (final (index, file) in files.indexed)
        file.copyWith(
          problem: () => index >= settings.maxAttachments
              ? StageProblem.tooMany
              : stageProblem(
                  name: file.name,
                  size: file.size,
                  limit: limit,
                  blocked: settings.blockedExtensions,
                ),
        ),
    ];
  }

  void _setStaged(List<StagedFile> staged) {
    _replaceStaged(_checked(staged));
    _startUploads();
  }

  void _replaceStaged(List<StagedFile> staged) => state = ComposerState(
    draft: state.draft,
    replyTo: state.replyTo,
    editing: state.editing,
    stashedDraft: state.stashedDraft,
    staged: staged,
  );

  /// Uploads each staged file that can be sent, as soon as it is staged
  /// (or its problem goes), so that sending is at once.
  void _startUploads() {
    final starting = {
      for (final file in state.staged)
        if (file.problem == null && file.upload == null) file.id,
    };
    if (starting.isEmpty) return;
    _replaceStaged([
      for (final file in state.staged)
        starting.contains(file.id)
            ? file.copyWith(upload: () => UploadPhase.preparing)
            : file,
    ]);
    for (final file in state.staged) {
      if (starting.contains(file.id)) unawaited(_upload(file));
    }
  }

  Future<void> _upload(StagedFile file) async {
    try {
      await _repository.stageUpload(channel.server, channel.channel, file);
    } on RepoException catch (error) {
      // Sending it uploads it again.
      if (!ref.mounted) return;
      _updateStaged(
        file.id,
        (file) => file.copyWith(
          upload: () => UploadPhase.failed,
          uploadError: () => error.message,
        ),
      );
    }
  }

  void _applyUpload(StagedUploadEvent event) => _updateStaged(
    event.stagedId,
    (file) => file.copyWith(
      upload: () => event.phase,
      uploadSent: event.sentBytes,
      uploadError: () => event.error,
    ),
  );

  void _updateStaged(String id, StagedFile Function(StagedFile) change) {
    if (!state.staged.any((file) => file.id == id)) return;
    _replaceStaged([
      for (final file in state.staged) file.id == id ? change(file) : file,
    ]);
  }

  /// Stages a file built by the caller; its id must differ from every
  /// staged file's.
  void stageFile(StagedFile file) => _setStaged([...state.staged, file]);

  /// Stages a pasted image.
  void stageBytes(String name, Uint8List bytes) => stageFile(
    StagedFile(
      id: _stagedId(),
      name: name,
      size: bytes.length,
      bytes: bytes,
      preview: bytes.length <= _previewMaxBytes ? bytes : null,
    ),
  );

  /// Stages files from the picker or a drop; folders and missing files are
  /// skipped.
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
      if (_previewExtensions.contains(fileExtension(name)) &&
          size <= _previewMaxBytes) {
        try {
          preview = await file.readAsBytes();
        } on FileSystemException {
          // Only the thumbnail is lost; sending reads the file again and
          // reports a failure there.
        }
      }
      files.add(
        StagedFile(
          id: _stagedId(),
          name: name,
          size: size,
          path: path,
          preview: preview,
        ),
      );
    }
    if (!ref.mounted) return;
    _setStaged([...state.staged, ...files]);
  }

  /// Removes a staged file, and stops or drops its upload.
  void removeStaged(String id) {
    final removed = state.staged.where((file) => file.id == id).firstOrNull;
    if (removed?.upload != null) {
      unawaited(
        _repository
            .discardStagedUpload(channel.server, id)
            .catchError((Object _) {}, test: (error) => error is RepoException),
      );
    }
    _setStaged([
      for (final file in state.staged)
        if (file.id != id) file,
    ]);
  }

  /// Empties the staged list once a message took the files: their uploads
  /// are the message's now.
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
