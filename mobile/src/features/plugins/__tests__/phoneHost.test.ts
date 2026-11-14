import { MobileEvent } from '@opencord/core-native';
import { Linking } from 'react-native';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import * as ui from '@/ui';

import { phoneHostDeps } from '../phoneHost';

jest.mock('@/ui', () => ({ ...jest.requireActual<object>('@/ui'), confirm: jest.fn() }));

async function ready() {
  freshStore();
  onEvents([new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) })]);
  await settle();
}

describe("the phone's host deps", () => {
  // Fails if names don't come from the server's members and channels.
  it('names come from the server', async () => {
    await ready();
    const deps = phoneHostDeps({ serverKey: NIGHT_OWLS, pluginId: 'tickets', openChannel: jest.fn() });
    const [member] = snapshot(NIGHT_OWLS).members;
    if (!member) throw new Error('the fake server has members');
    expect(deps.userName(member.user.id)).toBe(member.nickname ?? member.user.displayName);
    expect(deps.channelName(CHANNELS.general)).toBe('general');
    expect(deps.userName('999999')).toBeUndefined();
  });

  // Fails if the link question isn't asked with the table's words, the
  // link opened without a yes, or the answer lost.
  it('a link is opened only after Open', async () => {
    await ready();
    const confirm = ui.confirm as jest.Mock;
    const open = jest.spyOn(Linking, 'openURL').mockResolvedValue(undefined);
    const deps = phoneHostDeps({ serverKey: NIGHT_OWLS, pluginId: 'tickets', openChannel: jest.fn() });
    confirm.mockImplementationOnce((options: ui.ConfirmOptions) => options.onCancel?.());
    expect(await deps.confirmLink('https://opencord.dev')).toBe(false);
    expect(open).not.toHaveBeenCalled();
    confirm.mockImplementationOnce((options: ui.ConfirmOptions) => options.onConfirm());
    expect(await deps.confirmLink('https://opencord.dev')).toBe(true);
    expect(open).toHaveBeenCalledWith('https://opencord.dev');
    expect(confirm.mock.calls[0]?.[0]).toMatchObject({ title: 'Open this link?', message: 'https://opencord.dev', confirmTitle: 'Open' });
  });

  // Fails if a channel the viewer can't see is opened, or no toast says so.
  it('a channel the viewer can see is opened; another shows a toast', async () => {
    await ready();
    const openChannel = jest.fn();
    const deps = phoneHostDeps({ serverKey: NIGHT_OWLS, pluginId: 'tickets', openChannel });
    deps.openChannel(CHANNELS.general);
    expect(openChannel).toHaveBeenCalledWith(CHANNELS.general);
    deps.openChannel('424242');
    expect(openChannel).toHaveBeenCalledTimes(1);
    expect(useAppStore.getState().toasts.at(-1)?.text).toBe('This channel is unavailable');
  });
});
