// Plays voice cues as the scene changes in the store (spec §3 P1, spec §6.3).
import { useEffect } from 'react';

import { getCore } from '@/core';
import { useAppStore } from '@/state/store';
import type { AppState } from '@/state/types';

import { playVoiceCues, type CueScene } from './voiceCues';

export function buildCueScene(state: AppState): CueScene {
  const myVoice = state.myVoice;
  if (!myVoice) return {};

  const server = state.servers[myVoice.serverKey];
  const selfId = server?.snapshot?.selfUser?.id;
  const others = new Set<string>();

  for (const [userId, voiceState] of Object.entries(server?.voice ?? {})) {
    if (voiceState.channelId === myVoice.channelId && userId !== selfId) {
      others.add(userId);
    }
  }

  return {
    serverKey: myVoice.serverKey,
    channelId: myVoice.channelId,
    muted: myVoice.muted,
    deafened: myVoice.deafened,
    others,
    live: new Set(),
  };
}

/** Mounts once in the shell layout to play voice sounds on voice state transitions. */
export function useVoiceCues(): void {
  useEffect(() => {
    let prevScene: CueScene = buildCueScene(useAppStore.getState());

    return useAppStore.subscribe((state) => {
      const nextScene = buildCueScene(state);
      const cuesOff = state.prefs.cuesOff;
      playVoiceCues(prevScene, nextScene, cuesOff, (cue) => {
        try {
          getCore().cuePlay(cue, 1);
        } catch (error: unknown) {
          console.warn(`failed to play voice cue: ${error}`);
        }
      });
      prevScene = nextScene;
    });
  }, []);
}
