import { activeToken, decodeMentions, encodeMentions } from '../mentions';

const members = new Map<string, string>([
  ['1001', 'Kai Nakamura'],
  ['1002', 'Kai'],
  ['1003', 'Mira'],
]);

const channels = new Map<string, string>([
  ['10', 'general'],
  ['11', 'dev-core'],
]);

function encode(text: string): string {
  return encodeMentions(text, members, channels);
}

function decode(content: string): string {
  return decodeMentions(content, members, channels);
}

describe('encode', () => {
  // Fails if member and channel names are not replaced with tokens, or shorter names match first.
  it('names become mention tokens, the longest name first', () => {
    expect(encode('hi @Kai Nakamura and @Kai!')).toBe('hi <@1001> and <@1002>!');
    expect(encode('see #dev-core or #general.')).toBe('see <#11> or <#10>.');
  });

  // Fails if names match without word boundary checks.
  it('a name has to end at a word boundary', () => {
    expect(encode('@Miranda says hi')).toBe('@Miranda says hi');
    expect(encode('email@Mira')).toBe('email@Mira');
  });

  // Fails if text inside inline or block code is modified.
  it('code is left alone', () => {
    expect(encode('`@Mira` and @Mira')).toBe('`@Mira` and <@1003>');
    expect(encode('```\n@Mira #general\n```')).toBe('```\n@Mira #general\n```');
  });

  // Fails if unknown names are replaced or stripped.
  it('unknown names stay as typed', () => {
    expect(encode('@Nobody in #nowhere')).toBe('@Nobody in #nowhere');
  });
});

// Fails if IDs larger than 64-bit integers are parsed as valid mentions.
it('ids too big for 64 bits stay as typed', () => {
  const typed = '<@99999999999999999999> <#99999999999999999999>';
  expect(decode(typed)).toBe(typed);
});

// Fails if tokens are not replaced with names or if unknown tokens are replaced.
it('decode writes tokens back as names, for editing', () => {
  expect(decode('<@1001> see <#11>')).toBe('@Kai Nakamura see #dev-core');
  expect(decode('<@9> <#9>')).toBe('<@9> <#9>');
  expect(encode(decode('hi <@1003> in <#10>'))).toBe('hi <@1003> in <#10>');
});

// Fails if decoding a message with mentions and then encoding it changes the message.
it('decode then encode is the same message', () => {
  const original = 'Hello <@1001>, check <#10> and ask <@1003>!';
  expect(encode(decode(original))).toBe(original);
});

describe('active token', () => {
  // Fails if activeToken does not detect trigger, query, and start index at caret.
  it('finds what is being typed after @, # or :', () => {
    expect(activeToken('hello @Ka', 9)).toEqual({
      trigger: '@',
      query: 'Ka',
      start: 6,
    });
    expect(activeToken('#gen', 4)).toEqual({
      trigger: '#',
      query: 'gen',
      start: 0,
    });
    expect(activeToken('nice :thu', 9)).toEqual({
      trigger: ':',
      query: 'thu',
      start: 5,
    });
  });

  // Fails if member mention queries cannot contain spaces.
  it('member names may contain spaces', () => {
    expect(activeToken('@Kai Nak', 8)).toEqual({
      trigger: '@',
      query: 'Kai Nak',
      start: 0,
    });
  });

  // Fails if triggers in the middle of words or emoji queries with < 2 letters are accepted.
  it('needs a word start, and two letters for emoji', () => {
    expect(activeToken('a@b', 3)).toBeNull();
    expect(activeToken('at 10:30', 8)).toBeNull();
    expect(activeToken('nice :t', 7)).toBeNull();
    expect(activeToken('#gen eral', 9)).toBeNull();
  });

  // Fails if tokens cross newline boundaries.
  it('stops at a new line', () => {
    expect(activeToken('@Kai\nthere', 10)).toBeNull();
  });
});
