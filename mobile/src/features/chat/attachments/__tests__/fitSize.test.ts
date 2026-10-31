import { fitSize } from '../fitSize';

describe('fitSize', () => {
  it('fitSize keeps the aspect within 260 × 320', () => {
    // wide
    const wide = fitSize(800, 400);
    expect(wide.width).toBe(260);
    expect(wide.height).toBe(130);

    // tall
    const tall = fitSize(300, 600);
    expect(tall.width).toBe(160);
    expect(tall.height).toBe(320);

    // small
    const small = fitSize(100, 80);
    expect(small.width).toBe(100);
    expect(small.height).toBe(80);

    // zero sizes (zero gives 260 × 195)
    const zero = fitSize(0, 0);
    expect(zero).toEqual({ width: 260, height: 195 });
  });
});
