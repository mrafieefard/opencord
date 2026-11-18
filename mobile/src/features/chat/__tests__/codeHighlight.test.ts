import { highlightCode, type HighlightSpan } from '../codeHighlight';
import { tokens } from '@/ui/theme/tokens';

describe('Highlighted code blocks (Task 6)', () => {
  const darkColors = tokens.dark;

  // Fails if a rust block is not highlighted with keyword tokens.
  it('a rust block gets keyword spans', () => {
    const code = 'fn main() {\n    let x = 42;\n}';
    const spans = highlightCode(code, 'rust', darkColors);
    expect(spans).not.toBeNull();
    if (!spans) return;
    expect(spans.length).toBeGreaterThan(0);

    const fnSpan = spans.find((s: HighlightSpan) => s.text === 'fn');
    expect(fnSpan).toBeDefined();
    expect(fnSpan?.scope).toBe('keyword');
    expect(fnSpan?.style?.fontWeight).toBe('700');

    const letSpan = spans.find((s: HighlightSpan) => s.text === 'let');
    expect(letSpan).toBeDefined();
    expect(letSpan?.scope).toBe('keyword');
    expect(letSpan?.style?.fontWeight).toBe('700');

    // Number literal
    const numSpan = spans.find((s: HighlightSpan) => s.text === '42');
    expect(numSpan).toBeDefined();
    expect(numSpan?.style?.color).toBe(darkColors.textSecondary);

    // Also works with alias 'rs'
    const aliasSpans = highlightCode(code, 'rs', darkColors);
    expect(aliasSpans).not.toBeNull();
    if (!aliasSpans) return;
    const aliasFn = aliasSpans.find((s: HighlightSpan) => s.text === 'fn');
    expect(aliasFn?.scope).toBe('keyword');
  });

  // Fails if an unknown language attempts to highlight or returns spans instead of null.
  it('an unknown language stays plain', () => {
    const code = 'some random code here';
    expect(highlightCode(code, 'unknownlang123', darkColors)).toBeNull();
    expect(highlightCode(code, undefined, darkColors)).toBeNull();
    expect(highlightCode(code, '', darkColors)).toBeNull();
  });

  // Fails if blocks exceeding 400 lines or 20 KB are highlighted instead of staying plain.
  it('a huge block stays plain', () => {
    // 1. Exceeds 400 lines (401 lines)
    const lines = Array.from({ length: 401 }, (_, i) => `let v${i} = ${i};`).join('\n');
    expect(highlightCode(lines, 'rust', darkColors)).toBeNull();

    // 2. Exceeds 20 KB (20 * 1024 bytes = 20,480 bytes)
    const longCode = `fn main() {\n${' '.repeat(20 * 1024 + 10)}\n}`;
    expect(highlightCode(longCode, 'rust', darkColors)).toBeNull();

    // 3. Exactly 400 lines and < 20 KB is highlighted
    const exact400 = Array.from({ length: 400 }, (_, i) => `let x = ${i};`).join('\n');
    expect(highlightCode(exact400, 'rust', darkColors)).not.toBeNull();
  });
});
