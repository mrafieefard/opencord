import { act, fireEvent, render, screen } from '@testing-library/react-native';
import * as Clipboard from 'expo-clipboard';
import type { ReactElement } from 'react';
import { Share } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { getCore } from '@/core';
import { NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { ChannelListHeader } from '@/features/server/ChannelListHeader';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { InviteSheet } from '../InviteSheet';

const draw = (element: ReactElement) =>
  render(<GestureHandlerRootView style={{ flex: 1 }}>{element}</GestureHandlerRootView>);

describe('Invite people (Task 8)', () => {
  let shareSpy: jest.SpyInstance;
  let inviteCreateSpy: jest.SpyInstance;

  beforeEach(async () => {
    jest.clearAllMocks();
    freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    shareSpy = jest
      .spyOn(Share, 'share')
      .mockImplementation(async () => ({ action: 'sharedAction', activityType: undefined }));
    inviteCreateSpy = jest.spyOn(getCore(), 'inviteCreate');
  });

  afterEach(() => {
    shareSpy.mockRestore();
    inviteCreateSpy.mockRestore();
  });

  // Fails if the initial inviteCreate call does not use 604800s (7 days) and undefined maxUses.
  it('the defaults are 7 days and no limit', async () => {
    await draw(<InviteSheet serverKey={NIGHT_OWLS} />);
    await act(async () => {
      await settle();
    });

    expect(inviteCreateSpy).toHaveBeenCalledWith(NIGHT_OWLS, undefined, 604800);
    expect(screen.getByText('Invite people to Night Owls')).toBeTruthy();
    expect(screen.getByText('Anyone with this link can join Night Owls.')).toBeTruthy();
    expect(screen.getByText('opencord://fake.invite/AbC123')).toBeTruthy();
    expect(screen.getByText('7 days')).toBeTruthy();
    expect(screen.getByText('No limit')).toBeTruthy();
  });

  // Fails if changing expiry or limit does not call inviteCreate with new values.
  it('changing a choice makes a new link', async () => {
    await draw(<InviteSheet serverKey={NIGHT_OWLS} />);
    await act(async () => {
      await settle();
    });

    expect(inviteCreateSpy).toHaveBeenCalledTimes(1);

    // Change expiry to 1 day (86400)
    const expireMenu = screen.getByTestId('settings-choice-Expire after');
    await act(async () => {
      fireEvent(expireMenu, 'action', '86400');
    });

    expect(inviteCreateSpy).toHaveBeenCalledWith(NIGHT_OWLS, undefined, 86400);

    // Change max uses to 5 uses
    const maxUsesMenu = screen.getByTestId('settings-choice-Max uses');
    await act(async () => {
      fireEvent(maxUsesMenu, 'action', '5');
    });

    expect(inviteCreateSpy).toHaveBeenCalledWith(NIGHT_OWLS, 5, 86400);
  });

  // Fails if pressing Share does not pass the invite link to the system share sheet.
  it('share passes the link', async () => {
    await draw(<InviteSheet serverKey={NIGHT_OWLS} />);
    await act(async () => {
      await settle();
    });

    const shareButton = screen.getByRole('button', { name: 'Share' });
    await act(async () => {
      fireEvent.press(shareButton);
    });

    expect(shareSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        message: 'opencord://fake.invite/AbC123',
      }),
    );
  });

  // Fails if pressing Copy does not set clipboard or show toast.
  it('copy copies the link and shows a toast', async () => {
    await draw(<InviteSheet serverKey={NIGHT_OWLS} />);
    await act(async () => {
      await settle();
    });

    const copyButton = screen.getByRole('button', { name: 'Copy' });
    await act(async () => {
      fireEvent.press(copyButton);
    });

    expect(Clipboard.setStringAsync).toHaveBeenCalledWith('opencord://fake.invite/AbC123');
    const toasts = useAppStore.getState().toasts;
    expect(toasts.some((t) => t.text === 'Invite link copied')).toBe(true);
  });

  // Fails if ChannelListHeader does not show the person-plus button for permitted members.
  it('ChannelListHeader shows personAdd button when allowed and navigates', async () => {
    const { router } = jest.requireMock('expo-router');
    const onBack = jest.fn();
    await draw(<ChannelListHeader serverKey={NIGHT_OWLS} backLabel="Servers" onBack={onBack} />);

    const inviteBtn = screen.getByTestId('invite-button');
    expect(inviteBtn).toBeTruthy();

    await act(async () => {
      fireEvent.press(inviteBtn);
    });
    expect(router.push).toHaveBeenCalledWith(`/server/${NIGHT_OWLS}/invite`);
  });

  // Fails if ChannelListHeader shows the personAdd button when not allowed.
  it('ChannelListHeader hides personAdd button when not allowed', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(RUST_CLUB) }),
    ]);
    await settle();

    const onBack = jest.fn();
    await draw(<ChannelListHeader serverKey={RUST_CLUB} backLabel="Servers" onBack={onBack} />);

    expect(screen.queryByTestId('invite-button')).toBeNull();
  });
});
