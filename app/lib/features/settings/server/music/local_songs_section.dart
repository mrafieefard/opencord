import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Every song uploaded to the server, for administrators (design
/// 2026-10-24): searchable, newest first, each with a Remove button that
/// takes it off the server for everyone.
class LocalSongsSection extends ConsumerStatefulWidget {
  const LocalSongsSection({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<LocalSongsSection> createState() => _LocalSongsSectionState();
}

class _LocalSongsSectionState extends ConsumerState<LocalSongsSection> {
  /// Songs come 50 at a time.
  static const _page = 50;

  final _query = TextEditingController();
  Timer? _debounce;
  var _songs = <LocalSong>[];
  var _loading = true;
  var _more = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _query.dispose();
    super.dispose();
  }

  /// The first page for the query, or [before] the last song shown.
  Future<void> _load({int? before}) async {
    final query = _query.text.trim();
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final songs = await ref
          .read(repositoryProvider)
          .fetchLocalSongs(widget.serverKey, query: query, beforeId: before);
      if (!mounted || _query.text.trim() != query) return;
      setState(() {
        _songs = before == null ? songs : [..._songs, ...songs];
        _more = songs.length >= _page;
        _loading = false;
      });
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error.message;
        _loading = false;
      });
    }
  }

  void _search(String _) {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 300), _load);
  }

  Future<void> _remove(LocalSong song) async {
    final confirmed = await confirmAction(
      context,
      title: 'Remove ‘${song.title}’?',
      message:
          'It is removed from the server for everyone. Queues that hold it '
          'skip it.',
      action: 'Remove from server',
    );
    if (!confirmed || !mounted) return;
    try {
      await ref
          .read(repositoryProvider)
          .removeLocalSong(widget.serverKey, song.id);
      if (!mounted) return;
      setState(() => _songs = [..._songs.where((each) => each.id != song.id)]);
      showOcToast(context, '‘${song.title}’ was removed');
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final members =
        ref.watch(
          serverProvider(widget.serverKey).select((s) => s.data?.members),
        ) ??
        const {};
    String nameOf(int userId) => members[userId]?.displayName ?? 'Someone';
    final error = _error;
    return SettingsSection(
      title: 'Uploaded songs',
      footer:
          'Songs members dropped on the music player. This server plays them '
          'itself, without Lavalink. Members find their own in music search, '
          'or every song with Access all uploaded songs.',
      children: [
        Padding(
          padding: const EdgeInsets.all(OcSpace.s12),
          child: OcTextField(
            controller: _query,
            hint: 'Search uploaded songs',
            prefixIcon: OcIcons.search,
            maxLength: 200,
            semanticLabel: 'Search uploaded songs',
            onChanged: _search,
          ),
        ),
        if (error != null)
          Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: InlineError(error),
          )
        else if (_songs.isEmpty && !_loading)
          Padding(
            padding: const EdgeInsets.all(OcSpace.s16),
            child: Text(
              _query.text.trim().isEmpty
                  ? 'No songs have been uploaded yet.'
                  : 'No uploaded song matches.',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ),
        for (final song in _songs)
          Padding(
            key: ValueKey(song.id),
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s16,
              vertical: OcSpace.s10,
            ),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        song.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.body.copyWith(color: colors.text),
                      ),
                      Text(
                        [
                          if (song.artist.isNotEmpty) song.artist,
                          formatTrackTime(song.duration),
                          formatFileSize(song.sizeBytes),
                          'by ${nameOf(song.uploaderId)}',
                          longDate(song.createdAt),
                        ].join(' · '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(color: colors.textMuted),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: OcSpace.s12),
                OcButton(
                  label: 'Remove',
                  dense: true,
                  onPressed: () => _remove(song),
                ),
              ],
            ),
          ),
        if (_loading)
          const Padding(
            padding: EdgeInsets.all(OcSpace.s16),
            child: Center(child: OcSpinner(size: 16)),
          )
        else if (_more && _songs.isNotEmpty)
          Padding(
            padding: const EdgeInsets.all(OcSpace.s12),
            child: Center(
              child: OcButton(
                label: 'Show more',
                dense: true,
                onPressed: () => _load(before: _songs.last.id),
              ),
            ),
          ),
      ],
    );
  }
}
