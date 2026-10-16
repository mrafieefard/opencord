import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/music/music_queue.dart';

MusicQueueItem _item(String id, {int seconds = 180}) => MusicQueueItem(
  id: id,
  track: MusicTrack(
    title: id,
    artist: 'A',
    duration: Duration(seconds: seconds),
  ),
  requesterId: 1,
  addedAt: DateTime(2026),
);

void main() {
  test('the position moves on while playing, and stops at the end', () {
    final anchor = DateTime(2026, 10, 16, 12);
    final session = MusicSession(
      channelId: 1,
      state: MusicPlayState.playing,
      nowPlaying: _item('a', seconds: 60),
      position: const Duration(seconds: 10),
      anchor: anchor,
    );
    expect(
      session.positionAt(anchor.add(const Duration(seconds: 5))),
      const Duration(seconds: 15),
    );
    expect(
      session.positionAt(anchor.add(const Duration(minutes: 5))),
      const Duration(seconds: 60),
    );
    final paused = MusicSession(
      channelId: 1,
      state: MusicPlayState.paused,
      nowPlaying: _item('a'),
      position: const Duration(seconds: 10),
      anchor: anchor,
    );
    expect(
      paused.positionAt(anchor.add(const Duration(seconds: 30))),
      const Duration(seconds: 10),
    );
  });

  test('a move is sent as "before the item that follows"', () {
    final queue = [_item('a'), _item('b'), _item('c'), _item('d')];
    expect(beforeItemAfterMove(queue, 3, 0), 'a');
    expect(beforeItemAfterMove(queue, 0, 1), 'c');
    expect(beforeItemAfterMove(queue, 0, 3), isNull);
    expect(beforeItemAfterMove(queue, 2, 1), 'b');
  });

  test('times read as players show them', () {
    expect(formatTrackTime(const Duration(seconds: 187)), '3:07');
    expect(formatTrackTime(const Duration(seconds: 3727)), '1:02:07');
    expect(MusicRepeat.off.next, MusicRepeat.queue);
    expect(MusicRepeat.track.next, MusicRepeat.off);
  });

  test('the mock keeps the server queue rules', () {
    var now = DateTime(2026, 10, 16);
    final music = MockMusic(() => now);
    final found = music.search('song', 'youtube');
    final ids = [for (final result in found.results) result.id];
    var session = music.enqueue(7, 1, ids);
    expect(session.nowPlaying?.track.title, 'song 1');
    expect(session.queue.map((i) => i.track.title), ['song 2', 'song 3']);

    session = music.update(
      7,
      (player) => player.setMode(repeat: MusicRepeat.queue),
    );
    session = music.update(7, (player) => player.next());
    expect(session.nowPlaying?.track.title, 'song 2');
    expect(session.queue.last.track.title, 'song 1', reason: 'repeat queue');

    now = now.add(const Duration(seconds: 10));
    session = music.update(7, (player) => player.previous());
    expect(session.nowPlaying?.track.title, 'song 2', reason: 'restarted');

    expect(
      () => music.update(7, (player) => player.expect('gone')),
      throwsA(
        isA<RepoException>().having(
          (error) => error.kind,
          'kind',
          RepoErrorKind.musicStale,
        ),
      ),
    );
    final again = music.enqueue(
      7,
      1,
      const [],
      historyItemIds: [session.history.first.id],
      placement: MusicPlacement.now,
    );
    expect(again.nowPlaying?.track.title, 'song 1');

    session = music.update(7, (player) => player.voteSkip(10, again.nowPlaying!.id));
    expect(session.skipVotes, [10]);
    expect(session.nowPlaying?.track.title, 'song 1');

    session = music.update(7, (player) => player.voteSkip(11, session.nowPlaying!.id));
    expect(session.skipVotes, isEmpty);
    expect(session.nowPlaying?.track.title, 'song 3');
  });
}
