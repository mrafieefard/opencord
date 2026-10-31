import { render } from '@testing-library/react-native';

import { NIGHT_OWLS } from '@/core/fake/world';
import { freshStore } from '@/test/store';

import { MarkdownText } from '../MarkdownText';
import { bubbleCorners } from '../MessageBubble';

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
});
