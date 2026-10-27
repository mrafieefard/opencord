// Where the server screen is (spec §7.2): the channel list (`channels`) or a
// channel's pane (`content`), with the sidebar open over it or not. Back
// moves one step out; `false` means the server screen should close.
import { ChannelKind, type ChannelRecord, type ServerSnapshot } from '@opencord/core-native';
import { useCallback, useMemo, useRef, useState } from 'react';

import { useServer } from '@/state/selectors';

export type PaneMode = 'channels' | 'content';

export interface OpenChannel {
  id: string;
  kind: 'text' | 'voice';
}

export interface ServerPanes {
  mode: PaneMode;
  open: OpenChannel | null;
  sidebarOpen: boolean;
  select(channel: ChannelRecord): void;
  back(): boolean;
  openSidebar(): void;
  closeSidebar(): void;
}

interface PaneState {
  mode: PaneMode;
  openId: string | null;
  sidebarOpen: boolean;
}

const AT_CHANNELS: PaneState = { mode: 'channels', openId: null, sidebarOpen: false };

function viewableChannel(snapshot: ServerSnapshot, channelId: string): ChannelRecord | undefined {
  const channel = snapshot.channels.find((candidate) => candidate.id === channelId);
  const view = snapshot.access.find((entry) => entry.channelId === channelId)?.view ?? false;
  return channel && view ? channel : undefined;
}

/**
 * The state as it stands: until the server is ready its channels aren't
 * known, so a channel from the route waits for them. Once known, an open
 * channel that went away (or can no longer be seen) leaves for `channels`.
 */
function settled(state: PaneState, snapshot: ServerSnapshot | null): PaneState {
  if (state.openId === null || snapshot === null) return state;
  return viewableChannel(snapshot, state.openId) ? state : AT_CHANNELS;
}

export function useServerPanes(serverKey: string, initialChannelId?: string): ServerPanes {
  const snapshot = useServer(serverKey)?.snapshot ?? null;
  const [state, setState] = useState<PaneState>(() =>
    initialChannelId ? { mode: 'content', openId: initialChannelId, sidebarOpen: false } : AT_CHANNELS,
  );
  // The actions answer at once and may follow each other in one event, so
  // they read and write this rather than the last render's state.
  const latest = useRef(state);
  const update = useCallback((next: PaneState) => {
    latest.current = next;
    setState(next);
  }, []);
  const now = useCallback(() => settled(latest.current, snapshot), [snapshot]);

  const select = useCallback(
    (picked: ChannelRecord) => update({ mode: 'content', openId: picked.id, sidebarOpen: false }),
    [update],
  );

  const back = useCallback((): boolean => {
    const current = now();
    if (current.mode === 'channels') return false;
    update(current.sidebarOpen ? { ...current, sidebarOpen: false } : { ...current, mode: 'channels' });
    return true;
  }, [now, update]);

  const openSidebar = useCallback(() => {
    const current = now();
    if (current.mode === 'content' && !current.sidebarOpen) update({ ...current, sidebarOpen: true });
  }, [now, update]);

  const closeSidebar = useCallback(() => {
    const current = now();
    if (current.sidebarOpen) update({ ...current, sidebarOpen: false });
  }, [now, update]);

  const shown = settled(state, snapshot);
  const channel = shown.openId && snapshot ? viewableChannel(snapshot, shown.openId) : undefined;
  const openId = channel?.id;
  const openKind: OpenChannel['kind'] | undefined = channel
    ? channel.kind === ChannelKind.Voice
      ? 'voice'
      : 'text'
    : undefined;
  const open = useMemo(
    (): OpenChannel | null => (openId && openKind ? { id: openId, kind: openKind } : null),
    [openId, openKind],
  );

  return useMemo(
    () => ({ mode: shown.mode, open, sidebarOpen: shown.sidebarOpen, select, back, openSidebar, closeSidebar }),
    [shown.mode, shown.sidebarOpen, open, select, back, openSidebar, closeSidebar],
  );
}
