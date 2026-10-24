import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show KeepAliveLink;

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/providers/music_session_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';

/// A song dropped on the music player (design 2026-10-24), until it is
/// queued: checked against the server's songs, uploading, being checked by
/// the server, or failed.
@immutable
class MusicUpload {
  const MusicUpload({
    required this.id,
    required this.filename,
    this.sentBytes = 0,
    this.totalBytes = 0,
    this.phase = UploadPhase.preparing,
    this.error,
  });

  final String id;
  final String filename;
  final int sentBytes;
  final int totalBytes;
  final UploadPhase phase;
  final String? error;

  bool get failed => phase == UploadPhase.failed;

  /// 0 to 1 while uploading.
  double get progress =>
      totalBytes == 0 ? 0 : (sentBytes / totalBytes).clamp(0, 1).toDouble();

  MusicUpload copyWith({
    int? sentBytes,
    int? totalBytes,
    UploadPhase? phase,
    String? error,
  }) => MusicUpload(
    id: id,
    filename: filename,
    sentBytes: sentBytes ?? this.sentBytes,
    totalBytes: totalBytes ?? this.totalBytes,
    phase: phase ?? this.phase,
    error: error ?? this.error,
  );

  @override
  bool operator ==(Object other) =>
      other is MusicUpload &&
      other.id == id &&
      other.filename == filename &&
      other.sentBytes == sentBytes &&
      other.totalBytes == totalBytes &&
      other.phase == phase &&
      other.error == error;

  @override
  int get hashCode =>
      Object.hash(id, filename, sentBytes, totalBytes, phase, error);
}

/// A channel's dropped songs on their way into its queue, oldest first.
class MusicUploadsNotifier extends Notifier<List<MusicUpload>> {
  MusicUploadsNotifier(this.channel);

  final MusicChannel channel;
  var _next = 0;

  /// Held while songs are on their way, so closing the panel keeps them.
  KeepAliveLink? _busy;

  @override
  set state(List<MusicUpload> value) {
    super.state = value;
    if (value.isEmpty) {
      _busy?.close();
      _busy = null;
    } else {
      _busy ??= ref.keepAlive();
    }
  }

  @override
  List<MusicUpload> build() {
    final events = ref.watch(repositoryProvider).events.listen((event) {
      if (event is MusicUploadEvent &&
          event.serverKey == channel.server &&
          event.channelId == channel.channel) {
        _apply(event);
      }
    });
    ref.onDispose(events.cancel);
    return const [];
  }

  /// Queues the songs among [paths], in order; [playNow] plays the first
  /// at once. Returns the files that aren't songs, which are left out.
  List<String> drop(List<String> paths, {bool playNow = false}) {
    final songs = paths.where(isSongFile).toList();
    for (final (index, path) in songs.indexed) {
      final id = 'song-${DateTime.now().microsecondsSinceEpoch}-${_next++}';
      state = [
        ...state,
        MusicUpload(id: id, filename: path.split(RegExp(r'[/\\]')).last),
      ];
      unawaited(_upload(id, path, playNow: playNow && index == 0));
    }
    return [
      for (final path in paths)
        if (!isSongFile(path)) path,
    ];
  }

  Future<void> _upload(String id, String path, {required bool playNow}) async {
    try {
      await ref
          .read(repositoryProvider)
          .musicUploadSong(
            channel.server,
            channel.channel,
            uploadId: id,
            path: path,
            playNow: playNow,
          );
    } on RepoException catch (error) {
      if (!ref.mounted || error.kind == RepoErrorKind.cancelled) return;
      _update(
        id,
        (upload) =>
            upload.copyWith(phase: UploadPhase.failed, error: error.message),
      );
    }
  }

  void _apply(MusicUploadEvent event) {
    switch (event.phase) {
      case UploadPhase.done || UploadPhase.cancelled:
        dismiss(event.uploadId);
      case _:
        _update(
          event.uploadId,
          (upload) => upload.copyWith(
            sentBytes: event.sentBytes,
            totalBytes: event.totalBytes,
            phase: event.phase,
            error: event.error,
          ),
        );
    }
  }

  void _update(String id, MusicUpload Function(MusicUpload) change) {
    state = [
      for (final upload in state) upload.id == id ? change(upload) : upload,
    ];
  }

  /// Stops a song on its way; it leaves the list.
  void cancel(String id) {
    ref.read(repositoryProvider).musicCancelSong(channel.server, id);
    dismiss(id);
  }

  /// Takes a song off the list: a failed one, once read.
  void dismiss(String id) {
    state = [
      for (final upload in state)
        if (upload.id != id) upload,
    ];
  }
}

final musicUploadsProvider = NotifierProvider.autoDispose
    .family<MusicUploadsNotifier, List<MusicUpload>, MusicChannel>(
      MusicUploadsNotifier.new,
    );
