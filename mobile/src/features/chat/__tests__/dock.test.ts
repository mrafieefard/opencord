import { renderHook } from '@testing-library/react-native';

/** A 48 pt navigation bar (Android's three buttons) or home indicator. */
const WINDOW_BOTTOM = 48;

/** The dock's margin and keyboard lift as `os` builds them, the window's bottom inset 48. */
async function dockOn(os: 'ios' | 'android'): Promise<{ bottom: number; lift: number }> {
  let hooks: typeof import('../dock') | undefined;
  jest.isolateModules(() => {
    const insets = { top: 0, left: 0, right: 0, bottom: WINDOW_BOTTOM };
    jest.doMock('react-native-safe-area-context', () => ({
      initialWindowMetrics: { insets, frame: { x: 0, y: 0, width: 400, height: 900 } },
      useSafeAreaInsets: () => insets,
    }));
    jest.requireActual<typeof import('react-native')>('react-native').Platform.OS = os;
    hooks = jest.requireActual<typeof import('../dock')>('../dock');
  });
  if (!hooks) throw new Error('dock not loaded');
  const bottom = (await renderHook(hooks.useDockBottom)).result.current;
  const lift = (await renderHook(hooks.useDockLift)).result.current;
  return { bottom, lift };
}

describe('the chat dock', () => {
  it('on iOS stands on the home indicator, a little lower, and the keyboard lifts it less that', async () => {
    expect(await dockOn('ios')).toEqual({ bottom: 44, lift: 36 });
  });

  it('on Android stands just above the navigation bar the screen ends on, and the keyboard, which counts that bar, lifts it less the bar', async () => {
    expect(await dockOn('android')).toEqual({ bottom: 8, lift: WINDOW_BOTTOM });
  });
});
