import { fireEvent, render } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';
import { router } from 'expo-router';

import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';
import { onEvents } from '@/state/store';

import { MarkdownText } from '../MarkdownText';
import { MessageBubble, bubbleCorners } from '../MessageBubble';
import type { MessageBubbleItem } from '../items';

/** The text reserving the time's room: an em space, then the time. */
const ROOM = ' 9:41';

type Node = { type: string; children: (Node | string)[] | null } | string;

/** Each top-level text block, as the strings it renders, in order. */
function blocks(tree: unknown): string[][] {
  const root = tree as { children: Node[] };
  const strings = (node: Node): string[] =>
    typeof node === 'string' ? [node] : (node.children ?? []).flatMap((child) => strings(child));
  return root.children.map((child) => strings(child));
}

describe('bubble layout', () => {
  beforeEach(() => {
    freshStore();
  });

  it('a bubble joins its group with 6 pt corners on the author side only', () => {
    expect(bubbleCorners(false, 'single')).toEqual({
      borderTopRightRadius: 18,
      borderBottomRightRadius: 18,
      borderTopLeftRadius: 18,
      borderBottomLeftRadius: 18,
    });
    expect(bubbleCorners(false, 'middle')).toMatchObject({ borderTopLeftRadius: 6, borderBottomLeftRadius: 6, borderTopRightRadius: 18 });
    expect(bubbleCorners(true, 'first')).toMatchObject({ borderTopRightRadius: 18, borderBottomRightRadius: 6, borderTopLeftRadius: 18 });
    expect(bubbleCorners(true, 'last')).toMatchObject({ borderTopRightRadius: 6, borderBottomRightRadius: 18 });
  });

  it('the time gets room at the end of the last line, or a line of its own after code', async () => {
    // The room ends the last block, whatever comes before it.
    const plain = await render(<MarkdownText content="evening all" serverKey={NIGHT_OWLS} trailing="9:41" trailingColor="black" />);
    expect(blocks(plain.toJSON())).toEqual([['evening all', ROOM]]);

    const quoted = await render(<MarkdownText content={'> quoted\nafter'} serverKey={NIGHT_OWLS} trailing="9:41" trailingColor="black" />);
    expect(blocks(quoted.toJSON())).toEqual([['quoted'], ['after', ROOM]]);

    // After code, the room is a line of its own.
    const code = await render(<MarkdownText content={'```\nx = 1\n```'} serverKey={NIGHT_OWLS} trailing="9:41" trailingColor="black" />);
    expect(blocks(code.toJSON())).toEqual([['x = 1'], [ROOM]]);

    const none = await render(<MarkdownText content="plain" serverKey={NIGHT_OWLS} />);
    expect(blocks(none.toJSON())).toEqual([['plain']]);
  });

  // Fails if tapping author name or author avatar does not navigate to member profile.
  it('tapping author name and avatar navigates to member profile', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    const item: MessageBubbleItem = {
      type: 'message',
      id: '1',
      message: {
        id: '1',
        channelId: CHANNELS.general,
        authorId: '1001',
        content: 'Hello from Kai',
        createdAtMs: 1761500000000,
        attachments: [],
        editedAtMs: undefined,
        nonce: undefined,
      },
      position: 'single',
      own: false,
      showAuthor: true,
      showAvatar: true,
      mentionsMe: false,
    };

    const { getByText, getByLabelText } = await render(
      <MessageBubble item={item} serverKey={NIGHT_OWLS} maxWidth={400} />,
    );

    const nameBtn = getByText('Kai');
    fireEvent.press(nameBtn);
    expect(router.push).toHaveBeenCalledWith(`/server/${NIGHT_OWLS}/member/1001`);

    const avatarBtn = getByLabelText('Kai avatar');
    fireEvent.press(avatarBtn);
    expect(router.push).toHaveBeenCalledWith(`/server/${NIGHT_OWLS}/member/1001`);
  });
});
