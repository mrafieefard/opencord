import { act, renderHook } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent, type ChannelRecord } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { useServerPanes } from '../useServerPanes';

function channel(id: string): ChannelRecord {
  const found = snapshot(NIGHT_OWLS).channels.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`no channel ${id} in the fake world`);
  return found;
}

async function connected() {
  freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
  ]);
  await settle();
}

describe('useServerPanes', () => {
  it('entering a server starts at channels', async () => {
    await connected();
    const { result } = await renderHook(() => useServerPanes(NIGHT_OWLS));
    expect(result.current.mode).toBe('channels');
    expect(result.current.open).toBeNull();
    expect(result.current.sidebarOpen).toBe(false);
  });

  it('selecting a channel opens it and closes the drawer', async () => {
    await connected();
    const { result } = await renderHook(() => useServerPanes(NIGHT_OWLS, CHANNELS.general));
    await act(() => result.current.openSidebar());
    expect(result.current.sidebarOpen).toBe(true);

    await act(() => result.current.select(channel(CHANNELS.lounge)));
    expect(result.current.mode).toBe('content');
    expect(result.current.open).toEqual({ id: CHANNELS.lounge, kind: 'voice' });
    expect(result.current.sidebarOpen).toBe(false);

    await act(() => result.current.select(channel(CHANNELS.offTopic)));
    expect(result.current.open).toEqual({ id: CHANNELS.offTopic, kind: 'text' });
  });

  it('back from content goes to channels, and back from channels leaves', async () => {
    await connected();
    const { result } = await renderHook(() => useServerPanes(NIGHT_OWLS));
    await act(() => result.current.select(channel(CHANNELS.general)));

    let handled = false;
    await act(() => {
      handled = result.current.back();
    });
    expect(handled).toBe(true);
    expect(result.current.mode).toBe('channels');

    await act(() => {
      handled = result.current.back();
    });
    expect(handled).toBe(false);
    expect(result.current.mode).toBe('channels');
  });

  it('back with the sidebar open closes it', async () => {
    await connected();
    const { result } = await renderHook(() => useServerPanes(NIGHT_OWLS));
    await act(() => result.current.select(channel(CHANNELS.general)));
    await act(() => result.current.openSidebar());

    let handled = false;
    await act(() => {
      handled = result.current.back();
    });
    expect(handled).toBe(true);
    expect(result.current.sidebarOpen).toBe(false);
    expect(result.current.mode).toBe('content');
    expect(result.current.open).toEqual({ id: CHANNELS.general, kind: 'text' });
  });

  it('a channel parameter opens that channel at once', async () => {
    await connected();
    const { result } = await renderHook(() => useServerPanes(NIGHT_OWLS, CHANNELS.lounge));
    expect(result.current.mode).toBe('content');
    expect(result.current.open).toEqual({ id: CHANNELS.lounge, kind: 'voice' });
    expect(result.current.sidebarOpen).toBe(false);
  });

  it('a deleted open channel goes back to channels', async () => {
    await connected();
    const { result } = await renderHook(() => useServerPanes(NIGHT_OWLS));
    await act(() => result.current.select(channel(CHANNELS.offTopic)));
    await act(() => result.current.openSidebar());

    await act(() => {
      onEvents([new MobileEvent.ChannelDeleted({ serverKey: NIGHT_OWLS, channelId: CHANNELS.offTopic })]);
    });
    expect(result.current.mode).toBe('channels');
    expect(result.current.open).toBeNull();
    expect(result.current.sidebarOpen).toBe(false);

    let handled = true;
    await act(() => {
      handled = result.current.back();
    });
    expect(handled).toBe(false);
  });
});
