import { act, fireEvent, render, renderHook } from '@testing-library/react-native';
import { View } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { router } from 'expo-router';
import { ConnectionRecord, FailureKind, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import type { FakeCore } from '@/core/fake/FakeCore';
import { ChannelList } from '@/features/server/ChannelList';
import { IdentityChangedScreen } from '@/features/servers/IdentityChangedScreen';
import { useChatStale } from '@/state/selectors';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ChatPane } from '../ChatPane';
import { ConnectionBanner } from '../ConnectionBanner';

describe('The reconnecting banner and a changed identity (Task 7)', () => {
  let fakeCore: FakeCore;

  beforeEach(async () => {
    fakeCore = freshStore();
    await settle();
    jest.clearAllMocks();
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
        new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
      ]);
    });
    await settle();
  });

  // Fails if cached chat does not show 0.6 opacity or composer is not disabled when reconnecting.
  it('the cached chat stays greyed while reconnecting', async () => {
    // 1. Initially connected: not stale
    const hook = await renderHook(() => useChatStale(NIGHT_OWLS));
    expect(hook.result.current).toBe(false);

    // 2. Reconnecting: stale
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Reconnecting({ attempt: 1, retryInMs: 3000 }),
        }),
      ]);
    });
    await settle();
    expect(hook.result.current).toBe(true);

    const { getByTestId } = await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChatPane serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );
    const messagesContainer = getByTestId('chat-messages-container');
    expect(messagesContainer.props.style).toEqual(
      expect.arrayContaining([expect.objectContaining({ opacity: 0.6 })]),
    );

    // Composer input is disabled
    const composerInput = getByTestId('composer-input');
    expect(composerInput.props.editable).toBe(false);

    // Reconnecting banner is visible
    expect(getByTestId('connection-banner')).toBeTruthy();
  });

  // Fails if pressing "Retry now" does not invoke core.serverRetryNow.
  it('retry now retries', async () => {
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Reconnecting({ attempt: 2, retryInMs: 5000 }),
        }),
      ]);
    });
    await settle();

    const view = await render(
      <View>
        <ConnectionBanner serverKey={NIGHT_OWLS} />
      </View>,
    );
    const retryBtn = view.getByText('Retry now');
    await act(async () => {
      fireEvent.press(retryBtn);
    });
    await settle();

    expect(fakeCore.calls.some((c) => c.method === 'serverRetryNow' && c.args[0] === NIGHT_OWLS)).toBe(true);
  });

  // Fails if the identity screen opens multiple times for the same presented fingerprint or doesn't open.
  it('the identity screen opens once per change', async () => {
    // Emit FingerprintChanged
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Failed({
            reason: FailureKind.FingerprintChanged,
            message: 'fingerprint mismatch',
            expectedFingerprint: 'AA11BB22',
            presentedFingerprint: 'CC33DD44',
          }),
        }),
      ]);
    });
    await settle();

    const onSelect = jest.fn();
    const { rerender } = await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={onSelect} />
      </GestureHandlerRootView>,
    );
    await settle();

    // Navigated once
    expect(router.push).toHaveBeenCalledTimes(1);
    expect(router.push).toHaveBeenCalledWith(`/server/${NIGHT_OWLS}/identity-changed`);

    // Rerender with same state: does NOT push again
    await rerender(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={onSelect} />
      </GestureHandlerRootView>,
    );
    await settle();
    expect(router.push).toHaveBeenCalledTimes(1);

    // A new change with a different presented fingerprint opens again
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Failed({
            reason: FailureKind.FingerprintChanged,
            message: 'different fingerprint',
            expectedFingerprint: 'AA11BB22',
            presentedFingerprint: 'EE55FF66',
          }),
        }),
      ]);
    });
    await settle();
    expect(router.push).toHaveBeenCalledTimes(2);
  });

  // Fails if Forget server does not invoke serverRemove or leave the server.
  it('forget removes the server', async () => {
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Failed({
            reason: FailureKind.FingerprintChanged,
            message: 'fingerprint mismatch',
            expectedFingerprint: '1122334455667788',
            presentedFingerprint: '9900AABBCCDDEEFF',
          }),
        }),
      ]);
    });
    await settle();

    const view = await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <IdentityChangedScreen serverKey={NIGHT_OWLS} />
      </GestureHandlerRootView>,
    );
    const { getByText } = view;

    // Screen title and body
    expect(getByText('Server identity changed')).toBeTruthy();
    expect(getByText(/no longer shows the identity you trusted/)).toBeTruthy();

    // Fingerprints in mono (grouped in 4-character blocks)
    expect(getByText('1122 3344 5566 7788')).toBeTruthy();
    expect(getByText('9900 AABB CCDD EEFF')).toBeTruthy();

    // Press Forget server
    const forgetBtn = getByText('Forget server');
    await act(async () => {
      fireEvent.press(forgetBtn);
    });
    await settle();

    expect(fakeCore.calls.some((c) => c.method === 'serverRemove' && c.args[0] === NIGHT_OWLS)).toBe(true);
    expect(useAppStore.getState().servers[NIGHT_OWLS]).toBeUndefined();
    expect(router.back).toHaveBeenCalled();
  });

  // Fails if an identity change leaves the chat enabled or fails to grey it out (Review Focus 5).
  it('an identity change greys the chat and disables composer (Review Focus 5)', async () => {
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Failed({
            reason: FailureKind.FingerprintChanged,
            message: 'fingerprint mismatch',
            expectedFingerprint: 'AA11BB22',
            presentedFingerprint: 'CC33DD44',
          }),
        }),
      ]);
    });
    await settle();

    // 1. Chat must be stale
    const hook = await renderHook(() => useChatStale(NIGHT_OWLS));
    expect(hook.result.current).toBe(true);

    // 2. ChatPane rendered with messages at 0.6 opacity and composer disabled
    const { getByTestId } = await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChatPane serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );
    const messagesContainer = getByTestId('chat-messages-container');
    expect(messagesContainer.props.style).toEqual(
      expect.arrayContaining([expect.objectContaining({ opacity: 0.6 })]),
    );

    const composerInput = getByTestId('composer-input');
    expect(composerInput.props.editable).toBe(false);

    // 3. Banner displays identity changed and tap navigates
    const banner = getByTestId('connection-banner');
    expect(banner).toBeTruthy();
    router.push = jest.fn();
    fireEvent.press(banner);
    expect(router.push).toHaveBeenCalledWith(`/server/${NIGHT_OWLS}/identity-changed`);
  });

  // Fails if remounting ChannelList re-triggers identity screen navigation (Item 4).
  it('remounting ChannelList does not reopen identity screen (Item 4)', async () => {
    await act(async () => {
      onEvents([
        new MobileEvent.Connection({
          serverKey: NIGHT_OWLS,
          state: new ConnectionRecord.Failed({
            reason: FailureKind.FingerprintChanged,
            message: 'fingerprint mismatch',
            expectedFingerprint: 'AA11BB22',
            presentedFingerprint: 'CC33DD44',
          }),
        }),
      ]);
    });
    await settle();

    router.push = jest.fn();

    // First mount
    const view1 = await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />
      </GestureHandlerRootView>,
    );
    await settle();
    expect(router.push).toHaveBeenCalledTimes(1);

    // Unmount view1
    view1.unmount();
    await settle();

    // Second mount (fresh mount of ChannelList)
    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />
      </GestureHandlerRootView>,
    );
    await settle();

    // Must NOT have called push again
    expect(router.push).toHaveBeenCalledTimes(1);
  });
});
