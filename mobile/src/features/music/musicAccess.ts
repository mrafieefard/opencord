// Evaluates what actions the current user can perform on the music player (spec §8.7).
import type {
  ChannelAccess,
  MusicSessionRecord,
  MusicSettingsRecord,
} from '@opencord/core-native';

export interface ControlAccess {
  allowed: boolean;
  reason?: string;
  isVoteSkip?: boolean;
  voteSkipLabel?: string;
  /** "1/3": the votes so far, of those needed. */
  voteCount?: string;
}

export interface MusicAccessResult {
  playPause: ControlAccess;
  previous: ControlAccess;
  next: ControlAccess;
  seek: ControlAccess;
  shuffle: ControlAccess;
  repeat: ControlAccess;
  canAdd: boolean;
  canManageQueue: boolean;
}

export function musicAccess(
  access: ChannelAccess | undefined,
  session: MusicSessionRecord | null,
  settings: MusicSettingsRecord | null,
  inChannel: boolean,
  selfUserId?: string,
): MusicAccessResult {
  const canControl = !!access?.musicControl;
  const reasonNoControl = 'You need Control music for this.';

  const playPause: ControlAccess = canControl
    ? { allowed: true }
    : { allowed: false, reason: reasonNoControl };

  const previous: ControlAccess = canControl
    ? { allowed: true }
    : { allowed: false, reason: reasonNoControl };

  // Next: without control, becomes Vote to skip (x/y) when vote skip is enabled on the server
  let next: ControlAccess;
  if (canControl) {
    next = { allowed: true };
  } else if (settings?.voteSkip && session && session.skipVotesNeeded > 0) {
    const votes = session.skipVotes.length;
    const needed = session.skipVotesNeeded;
    const hasVoted = selfUserId ? session.skipVotes.includes(selfUserId) : false;
    next = {
      allowed: !hasVoted,
      isVoteSkip: true,
      voteSkipLabel: `Vote to skip (${votes}/${needed})`,
      voteCount: `${votes}/${needed}`,
      reason: hasVoted ? 'You already voted to skip' : undefined,
    };
  } else {
    next = { allowed: false, reason: reasonNoControl };
  }

  // Seeking: needs a seekable track and music_control
  let seek: ControlAccess;
  if (!canControl) {
    seek = { allowed: false, reason: reasonNoControl };
  } else if (!session?.nowPlaying) {
    seek = { allowed: false, reason: 'Nothing is playing' };
  } else if (session.nowPlaying.track.isStream) {
    seek = { allowed: false, reason: "Live streams can't be seeked" };
  } else if (!session.nowPlaying.track.isSeekable) {
    seek = { allowed: false, reason: "This track can't be seeked" };
  } else {
    seek = { allowed: true };
  }

  const shuffle: ControlAccess = canControl
    ? { allowed: true }
    : { allowed: false, reason: reasonNoControl };

  const repeat: ControlAccess = canControl
    ? { allowed: true }
    : { allowed: false, reason: reasonNoControl };

  return {
    playPause,
    previous,
    next,
    seek,
    shuffle,
    repeat,
    canAdd: !!access?.musicAdd,
    canManageQueue: !!access?.musicManageQueue,
  };
}
