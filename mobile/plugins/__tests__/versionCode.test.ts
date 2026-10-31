import { versionCode } from '../versionCode';

describe('versionCode', () => {
  it('version codes keep the release order', () => {
    expect(versionCode('0.0.2-beta.3')).toBe(2003);
    expect(versionCode('0.0.2-canary.1')).toBe(2101);
    expect(versionCode('0.0.2')).toBe(2900);
    expect(versionCode('0.0.3-beta.1')).toBe(3001);
    expect(versionCode('1.2.3-canary.45')).toBe(10203145);
    const order = ['0.0.2-beta.3', '0.0.2-beta.12', '0.0.2-canary.1', '0.0.2', '0.0.3-beta.1', '0.1.0-beta.1', '1.0.0'];
    const codes = order.map(versionCode);
    expect([...codes].sort((a, b) => a - b)).toEqual(codes);
  });

  it('a pre-release number over 99 throws', () => {
    expect(() => versionCode('0.0.2-beta.100')).toThrow(/99/);
  });

  it('a minor or patch over 99 throws, since it would run into the next part', () => {
    expect(() => versionCode('0.100.0')).toThrow(/99/);
    expect(() => versionCode('0.0.100')).toThrow(/99/);
  });

  it('anything but a release version throws', () => {
    expect(() => versionCode('0.0.2-alpha.1')).toThrow(/not a release version/);
    expect(() => versionCode('v0.0.2')).toThrow(/not a release version/);
    expect(() => versionCode('')).toThrow(/not a release version/);
  });

  it("stays under Google Play's largest version code", () => {
    expect(versionCode('209.99.99')).toBeLessThanOrEqual(2_100_000_000);
    expect(() => versionCode('210.0.0')).toThrow(/2100000000/);
  });
});
