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
    // Nothing until a call is opened.
    expect(screen.queryByTestId('call-sheet', { includeHiddenElements: true })).toBeNull();

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
});
