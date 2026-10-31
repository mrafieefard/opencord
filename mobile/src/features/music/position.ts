// Desktop's positionAt in music_session.dart (spec §8.7):
// Moving on while playing, never past the track's end.
import { MusicPlayState, type MusicSessionRecord } from '@opencord/core-native';

export function positionAt(session: MusicSessionRecord, nowMs: number): number {
  const anchor = session.anchorMs;
  let at = session.positionMs;
  if (session.state === MusicPlayState.Playing && anchor != null && nowMs > anchor) {
    at += nowMs - anchor;
  }
  const length = session.nowPlaying?.track.durationMs ?? 0;
  return length > 0 && at > length ? length : at;
}

export function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}
