// The phone's half of a plugin area's host calls (Task 9's table): names
// and avatars from the store, the pickers, the app's question before a
// link, the clipboard, channel navigation, and sounds through expo-audio.
import { createAudioPlayer } from 'expo-audio';
import * as Clipboard from 'expo-clipboard';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import { Linking } from 'react-native';

import { getCore } from '@/core';
import { filePath } from '@/core/paths';
import { toast } from '@/state/actions/ui';
import { memberName } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { confirm } from '@/ui';

import type { HostCallDeps } from './hostCalls';

/** A sound that never says it ended still lets its `on_end` run. */
const SOUND_LONGEST_MS = 5 * 60 * 1000;

export interface PhoneHostOptions {
  serverKey: string;
  pluginId: string;
  /** The server screen's own navigation to a channel. */
  openChannel: (id: string) => void;
}

function server(serverKey: string) {
  return useAppStore.getState().servers[serverKey];
}

function playSound(path: string, volume: number): Promise<void> {
  return new Promise((resolve) => {
    let player: ReturnType<typeof createAudioPlayer> | null = null;
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      clearTimeout(longest);
      try {
        player?.remove();
      } catch {
        // Already gone.
      }
      resolve();
    };
    const longest = setTimeout(finish, SOUND_LONGEST_MS);
    try {
      player = createAudioPlayer(path);
      player.volume = volume;
      player.addListener('playbackStatusUpdate', (status) => {
        if (status.didJustFinish) finish();
      });
      player.play();
    } catch (error: unknown) {
      console.warn(`plugin sound failed: ${error instanceof Error ? error.name : typeof error}`);
      finish();
    }
  });
}

export function phoneHostDeps({ serverKey, pluginId, openChannel }: PhoneHostOptions): HostCallDeps {
  return {
    core: getCore(),
    serverKey,
    pluginId,
    userName: (id) => memberName(server(serverKey)?.snapshot ?? null, id),
    userAvatarHash: (id) => {
      const state = server(serverKey);
      const member = state?.snapshot?.members.find((candidate) => candidate.user.id === id);
      if (member) return member.user.avatarHash ?? null;
      const plugin = state?.pluginUsers?.[id] ?? state?.snapshot?.pluginUsers?.find((candidate) => candidate.userId === id);
      return plugin?.avatarHash ?? null;
    },
    channelName: (id) => server(serverKey)?.snapshot?.channels.find((channel) => channel.id === id)?.name,
    pickImage: async () => {
      const picked = await ImagePicker.launchImageLibraryAsync({ mediaTypes: ImagePicker.MediaTypeOptions.Images });
      const asset = picked.canceled ? undefined : picked.assets[0];
      return asset ? filePath(asset.uri) : null;
    },
    pickDocument: async (accept) => {
      const picked = await DocumentPicker.getDocumentAsync({ type: accept.length > 0 ? accept : '*/*' });
      const asset = picked.canceled ? undefined : picked.assets[0];
      return asset ? filePath(asset.uri) : null;
    },
    confirmLink: (url) =>
      new Promise((resolve) => {
        confirm({
          title: 'Open this link?',
          message: url,
          confirmTitle: 'Open',
          onConfirm: () => {
            Linking.openURL(url).then(
              () => resolve(true),
              () => resolve(false),
            );
          },
          onCancel: () => resolve(false),
        });
      }),
    copy: async (text) => {
      await Clipboard.setStringAsync(text);
      toast('Copied');
    },
    openChannel: (id) => {
      const snapshot = server(serverKey)?.snapshot;
      const seen = snapshot?.access.find((entry) => entry.channelId === id)?.view ?? false;
      if (snapshot?.channels.some((channel) => channel.id === id) && seen) openChannel(id);
      else toast('This channel is unavailable');
    },
    playSound,
  };
}
