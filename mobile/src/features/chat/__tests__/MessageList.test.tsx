import { fireEvent, render } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent, type MessageRecord } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';
import { onEvents, useAppStore } from '@/state/store';
import { channelKey } from '@/state/types';

import { MessageList } from '../MessageList';
import { TypingLine } from '../TypingLine';

function msg(
  id: string,
  authorId: string,
  content: string,
  createdAtMs: number = Date.now(),
  channelId = CHANNELS.general,
): MessageRecord {
  return {
    id,
    channelId,
    authorId,
    content,
    createdAtMs,
    attachments: [],
    editedAtMs: undefined,
    nonce: undefined,
  };
}

describe('MessageList', () => {
  const KAI = '1001';
  const MIRA = '1002';

  beforeEach(async () => {
    freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();
  });

  it('names show on the first bubble of a group and avatars beside the last', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    const baseTime = 1761500000000;
    useAppStore.setState({
      messages: {
        [key]: {
          items: [
            msg('1', KAI, 'first message from Kai', baseTime),
            msg('2', KAI, 'second message from Kai', baseTime + 60000),
            msg('3', KAI, 'third message from Kai', baseTime + 120000),
          ],
          hasOlder: false,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
      openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: true },
    });

    const { getAllByText, getAllByTestId } = await render(
      <MessageList serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />,
    );

    // Author name Kai should only appear once (on first bubble)
    const kaiNames = getAllByText('Kai');
    expect(kaiNames).toHaveLength(1);

    // Avatar for Kai should be present on the last bubble
    const avatars = getAllByTestId('avatar-ring');
    expect(avatars).toHaveLength(1);
  });

  it('a mention shows the member\'s name', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    useAppStore.setState({
      messages: {
        [key]: {
          items: [
            msg('1', KAI, `Hello <@${MIRA}> welcome!`, 1761500000000),
          ],
          hasOlder: false,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
      openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: true },
    });

    const { getByText } = await render(
      <MessageList serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />,
    );

    // Mentions render as @Mira
    expect(getByText('@Mira')).toBeTruthy();
  });

  it('a channel link opens that channel', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    useAppStore.setState({
      messages: {
        [key]: {
          items: [
            msg('1', KAI, `Check out <#${CHANNELS.offTopic}>`, 1761500000000),
          ],
          hasOlder: false,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
      openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: true },
    });

    const onSelectChannel = jest.fn();
    const { getByText } = await render(
      <MessageList
        serverKey={NIGHT_OWLS}
        channelId={CHANNELS.general}
        onSelectChannel={onSelectChannel}
      />,
    );

    const channelLink = getByText('#off-topic');
    expect(channelLink).toBeTruthy();

    fireEvent.press(channelLink);
    expect(onSelectChannel).toHaveBeenCalledWith(CHANNELS.offTopic);
  });

  it('jump to latest appears when scrolled up and counts what\'s new', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    useAppStore.setState({
      messages: {
        [key]: {
          items: [
            msg('200001', KAI, 'message 1', 1761500000000),
            msg('200002', KAI, 'message 2', 1761500001000),
          ],
          hasOlder: false,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
      openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: false },
    });

    // Simulate an unread message coming in while scrolled up
    onEvents([
      new MobileEvent.MessageCreated({
        serverKey: NIGHT_OWLS,
        message: msg('200003', KAI, 'new unread message', 1761500002000),
      }),
    ]);

    const { getByTestId, getByText } = await render(
      <MessageList serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />,
    );

    const jumpButton = getByTestId('jump-to-latest');
    expect(jumpButton).toBeTruthy();
    expect(getByText('1')).toBeTruthy();
  });

  it('typing reads as one name, two names, or several people', async () => {
    // 1 person typing
    const { rerender, getByText } = await render(<TypingLine names={['Kai']} />);
    expect(getByText('Kai is typing…')).toBeTruthy();

    // 2 people typing
    await rerender(<TypingLine names={['Kai', 'Mira']} />);
    expect(getByText('Kai and Mira are typing…')).toBeTruthy();

    // 3+ people typing
    await rerender(<TypingLine names={['Kai', 'Mira', 'Sam']} />);
    expect(getByText('Several people are typing…')).toBeTruthy();
  });
});
