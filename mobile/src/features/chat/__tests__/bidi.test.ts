import { textDirectionOf } from '../bidi';

describe('textDirectionOf', () => {
  it('goes by the first letter', () => {
    expect(textDirectionOf('سلام، how are you')).toBe('rtl');
    expect(textDirectionOf('hello سلام')).toBe('ltr');
    expect(textDirectionOf('שלום')).toBe('rtl');
  });

  it('skips digits, spaces and symbols before the first letter', () => {
    expect(textDirectionOf('12:30 — سلام')).toBe('rtl');
    expect(textDirectionOf('  ** hi')).toBe('ltr');
  });

  it('has no direction without a letter', () => {
    expect(textDirectionOf('')).toBeNull();
    expect(textDirectionOf('123 !?')).toBeNull();
    expect(textDirectionOf('👍')).toBeNull();
  });
});
