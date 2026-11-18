// Plays voice cues as the scene changes in the store (spec §3 P1, spec §6.3).
import { useEffect } from 'react';

import { VoiceCueKind } from '@opencord/core-native';
import { getCore } from '@/core';
import { useAppStore } from '@/state/store';
import type { AppState } from '@/state/types';

import { consumeLeaveCueHandled, playVoiceCues, type CueScene } from './voiceCues';

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
    let pendingMuteTimeout: ReturnType<typeof setTimeout> | null = null;

    const unsubscribe = useAppStore.subscribe((state) => {
      const nextScene = buildCueScene(state);
      const cuesOff = state.prefs.cuesOff;
      const skipLeave = !nextScene.channelId && prevScene.channelId && consumeLeaveCueHandled();

      playVoiceCues(prevScene, nextScene, cuesOff, (cue) => {
        if (cue === VoiceCueKind.Leave && skipLeave) {
          return;
        }

        if (cue === VoiceCueKind.Mute) {
          if (pendingMuteTimeout) clearTimeout(pendingMuteTimeout);
          pendingMuteTimeout = setTimeout(() => {
            pendingMuteTimeout = null;
            try {
              getCore().cuePlay(VoiceCueKind.Mute, 1);
            } catch (error: unknown) {
              console.warn(`failed to play voice cue: ${error}`);
            }
          }, 50);
          return;
        }

        if (cue === VoiceCueKind.Deafen) {
          if (pendingMuteTimeout) {
            clearTimeout(pendingMuteTimeout);
            pendingMuteTimeout = null;
          }
        }

        try {
          getCore().cuePlay(cue, 1);
        } catch (error: unknown) {
          console.warn(`failed to play voice cue: ${error}`);
        }
      });
      prevScene = nextScene;
    });

    return () => {
      if (pendingMuteTimeout) clearTimeout(pendingMuteTimeout);
      unsubscribe();
    };
  }, []);
}
