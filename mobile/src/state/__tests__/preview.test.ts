import { previewText } from '../preview';

const lookup = {
  member: (id: string) => ({ '1001': 'Kai' })[id],
  channel: (id: string) => ({ '10': 'general' })[id],
};

describe('previewText strips the markdown subset and resolves mentions and channel links', () => {
  it.each([
    ['**bold** _it_ ~~gone~~ `code`', 'bold it gone code'],
    ['*star* and snake_case_name', 'star and snake_case_name'],
    ['> quoted', 'quoted'],
    ['```\nlet x = 1;\n```', 'let x = 1;'],
    ['<@1001> hi', '@Kai hi'],
    ['see <#10>', 'see #general'],
    ['<@42> in <#99>', '@unknown in #unknown'],
    ['two\nlines', 'two lines'],
  ])('%s', (input, expected) => {
    expect(previewText(input, lookup)).toBe(expected);
  });
});
