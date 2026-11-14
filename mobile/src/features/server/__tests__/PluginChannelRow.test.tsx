import { fireEvent, render } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent, PluginChannelState } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ChannelList } from '../ChannelList';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn() },
}));

describe('plugin channel rows', () => {
  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  /** Night Owls, with Old board's plugin in `state`. */
  async function readyWith(oldBoard: PluginChannelState) {
    const base = snapshot(NIGHT_OWLS);
    const channels = base.channels.map((channel) =>
      channel.id === CHANNELS.oldBoard && channel.plugin ? { ...channel, plugin: { ...channel.plugin, state: oldBoard } } : channel,
    );
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: { ...base, channels } }),
    ]);
    await settle();
  }

  // Fails if the row lacks the type's icon (the plugin's asset), or shows
  // an unread dot (§14.5: plugin channels have none).
  it("a plugin channel row shows its type's icon and no unread dot", async () => {
    await readyWith(PluginChannelState.PluginStopped);
    const { getByText, findByTestId, queryAllByTestId } = await render(
      <ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />,
    );
    expect(getByText('help-desk')).toBeTruthy();
    const icon = await findByTestId(`plugin-icon-${CHANNELS.helpDesk}`);
    expect(icon.props.source).toMatchObject({ uri: expect.stringContaining('/') });
    expect(queryAllByTestId('channel-unread-pill')).toHaveLength(0);
  });

  // Fails if an unavailable channel opens, or the toast doesn't say why
  // (§14.4: a stopped plugin, or a type the plugin no longer provides).
  it.each([
    [PluginChannelState.PluginStopped, 'This channel is unavailable'],
    [PluginChannelState.TypeGone, 'This plugin no longer provides this channel'],
  ])('tapping an unavailable channel says why and opens nothing (%s)', async (state, words) => {
    await readyWith(state);
    const onSelect = jest.fn();
    const { getByText } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={onSelect} />);
    await fireEvent.press(getByText('old-board'));
    expect(onSelect).not.toHaveBeenCalled();
    expect(useAppStore.getState().toasts.at(-1)?.text).toBe(words);
    await fireEvent.press(getByText('help-desk'));
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: CHANNELS.helpDesk }));
  });
});
