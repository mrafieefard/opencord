import { act, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { CallSheet } from '../CallSheet';
import { callProgress, closeCall, openCall } from '../callPosition';

async function inLounge(): Promise<FakeCore> {
  const core = freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
  ]);
  await settle();
  await core.voiceJoin(NIGHT_OWLS, CHANNELS.lounge);
  onEvents(await core.nextEvents(core.eventsAttach(), 100));
  return core;
}

describe('the call sheet', () => {
  it('shows the call this device is in, with its controls, and goes when asked', async () => {
    await inLounge();
    await render(
      <GestureHandlerRootView>
        <CallSheet />
      </GestureHandlerRootView>,
    );
    // Mounted, unseen, for as long as the call lasts: ready to grow at once.
    expect(screen.getByTestId('call-sheet', { includeHiddenElements: true })).toBeTruthy();
    expect(callProgress.get()).toBe(0);

    await act(async () => {
      openCall(NIGHT_OWLS, CHANNELS.lounge);
    });
    // Whether it is up comes from the UI thread, which tests don't run.
    const hidden = { includeHiddenElements: true };
    expect(screen.getByTestId('call-sheet', hidden)).toBeTruthy();
    expect(screen.getByTestId('control-leave', hidden)).toBeTruthy();
    expect(callProgress.get()).toBe(1);

    // The chevron puts it away (its touches need the UI thread's "up").
    expect(screen.getByLabelText('Close', hidden)).toBeTruthy();
    await act(async () => {
      closeCall();
    });
    expect(callProgress.get()).toBe(0);
  });

  it('leaving the call puts it away', async () => {
    const core = await inLounge();
    await render(
      <GestureHandlerRootView>
        <CallSheet />
      </GestureHandlerRootView>,
    );
    await act(async () => {
      openCall(NIGHT_OWLS, CHANNELS.lounge);
    });
    await act(async () => {
      await core.voiceLeave();
      onEvents(await core.nextEvents(core.eventsAttach(), 100));
    });
    expect(useAppStore.getState().myVoice).toBeNull();
    expect(callProgress.get()).toBe(0);
  });

  // Jest runs as iOS, where the pill above the tab bar is the call's own glass.
  it('rests as the voice pill above the tab bar or in a chat, and not while a chat has no room for it', async () => {
    await inLounge();
    useAppStore.setState({ tabInset: 83 });
    await render(
      <GestureHandlerRootView>
        <CallSheet />
      </GestureHandlerRootView>,
    );
    // At rest the row is the pill's: it can be tapped, muted, left.
    expect(screen.getByTestId('connected-bar-pressable')).toBeTruthy();
    expect(screen.getByTestId('connected-bar-mute')).toBeTruthy();
    expect(screen.queryByTestId('control-leave')).toBeNull();

    // A chat in front hides the tab bar: with no room kept, the pill goes.
    await act(async () => {
      useAppStore.setState({ chatInFront: true });
    });
    expect(screen.queryByTestId('connected-bar-pressable')).toBeNull();

    // The room the chat keeps above its composer takes it.
    await act(async () => {
      useAppStore.setState({ voiceSlot: { y: 700, height: 56 } });
    });
    expect(screen.getByTestId('connected-bar-pressable')).toBeTruthy();
  });
});
