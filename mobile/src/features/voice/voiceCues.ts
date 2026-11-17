// Voice sounds (cues) state transition and dispatch (spec §3 P1, spec §6.3).
// Port of the desktop's pure cuesBetween function (app/lib/core/providers/voice_cues.dart).
import { VoiceCueKind } from '@opencord/core-native';
import type { CueGroup } from '@/state/types';

export interface CueGroupInfo {
  readonly id: CueGroup;
  readonly label: string;
  readonly cues: readonly VoiceCueKind[];
}

export const CUE_GROUPS: readonly CueGroupInfo[] = [
  { id: 'joinLeave', label: 'Joining and leaving', cues: [VoiceCueKind.Join, VoiceCueKind.Leave] },
  { id: 'othersJoinLeave', label: 'Others joining and leaving', cues: [VoiceCueKind.UserJoin, VoiceCueKind.UserLeave] },
  { id: 'mute', label: 'Mute and unmute', cues: [VoiceCueKind.Mute, VoiceCueKind.Unmute] },
  { id: 'deafen', label: 'Deafen and undeafen', cues: [VoiceCueKind.Deafen, VoiceCueKind.Undeafen] },
  { id: 'stream', label: 'Streams starting and ending', cues: [VoiceCueKind.StreamStart, VoiceCueKind.StreamStop] },
] as const;

export interface CueScene {
  readonly serverKey?: string | null;
  readonly channelId?: string | number | null;
  readonly muted?: boolean;
  readonly deafened?: boolean;
  readonly others?: ReadonlySet<string | number>;
  readonly live?: ReadonlySet<string | number>;
}

/**
 * Returns the voice sounds that transitioning from `before` to `after` produces,
 * matching desktop cuesBetween exactly.
 */
export function cuesBetween(before: CueScene, after: CueScene): VoiceCueKind[] {
  const wasIn = before.channelId != null;
  const isIn = after.channelId != null;
  if (!wasIn && !isIn) return [];
  if (!isIn) return [VoiceCueKind.Leave];
  if (!wasIn || before.channelId !== after.channelId || before.serverKey !== after.serverKey) {
    return [VoiceCueKind.Join];
  }

  const cues: VoiceCueKind[] = [];
  if (Boolean(after.deafened) !== Boolean(before.deafened)) {
    cues.push(after.deafened ? VoiceCueKind.Deafen : VoiceCueKind.Undeafen);
  } else if (Boolean(after.muted) !== Boolean(before.muted)) {
    cues.push(after.muted ? VoiceCueKind.Mute : VoiceCueKind.Unmute);
  }

  const beforeOthers = before.others ?? new Set();
  const afterOthers = after.others ?? new Set();
  let userJoined = false;
  for (const user of afterOthers) {
    if (!beforeOthers.has(user)) {
      userJoined = true;
      break;
    }
  }
  if (userJoined) {
    cues.push(VoiceCueKind.UserJoin);
  }

  let userLeft = false;
  for (const user of beforeOthers) {
    if (!afterOthers.has(user)) {
      userLeft = true;
      break;
    }
  }
  if (userLeft) {
    cues.push(VoiceCueKind.UserLeave);
  }

  const beforeLive = before.live ?? new Set();
  const afterLive = after.live ?? new Set();
  let streamStarted = false;
  for (const user of afterLive) {
    if (!beforeLive.has(user)) {
      streamStarted = true;
      break;
    }
  }
  if (streamStarted) {
    cues.push(VoiceCueKind.StreamStart);
  }

  let streamStopped = false;
  for (const user of beforeLive) {
    if (!afterLive.has(user)) {
      streamStopped = true;
      break;
    }
  }
  if (streamStopped) {
    cues.push(VoiceCueKind.StreamStop);
  }

  return cues;
}

/**
 * Dispatches each cue produced between `before` and `after`, filtering out
 * groups disabled in `cuesOff`.
 */
export function playVoiceCues(
  before: CueScene,
  after: CueScene,
  cuesOff: readonly CueGroup[],
  play: (cue: VoiceCueKind) => void,
): void {
  const cues = cuesBetween(before, after);
  const offCues = new Set<VoiceCueKind>();
  for (const group of CUE_GROUPS) {
    if (cuesOff.includes(group.id)) {
      for (const cue of group.cues) {
        offCues.add(cue);
      }
    }
  }

  for (const cue of cues) {
    if (!offCues.has(cue)) {
      play(cue);
    }
  }
}
