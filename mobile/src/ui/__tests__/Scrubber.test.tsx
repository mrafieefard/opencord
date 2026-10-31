import { act, fireEvent, render } from '@testing-library/react-native';
import * as Haptics from 'expo-haptics';
import { GestureHandlerRootView, State } from 'react-native-gesture-handler';
import { fireGestureHandler, getByGestureTestId } from 'react-native-gesture-handler/jest-utils';

import { Scrubber, type ScrubberProps } from '../Scrubber';
import { ThemeProvider } from '../theme/ThemeProvider';

jest.mock('expo-haptics', () => ({ selectionAsync: jest.fn(async () => {}) }));

const WIDTH = 200;

async function renderScrubber(props: Partial<ScrubberProps> = {}) {
  const onChange = jest.fn();
  const onCommit = jest.fn();
  const view = await render(
    <GestureHandlerRootView>
      <ThemeProvider>
        <Scrubber
          testID="s"
          value={50}
          max={100}
          step={1}
          onChange={onChange}
          onCommit={onCommit}
          accessibilityLabel="Volume"
          accessibilityValue="50"
          {...props}
        />
      </ThemeProvider>
    </GestureHandlerRootView>,
  );
  await fireEvent(view.getByTestId('s'), 'layout', { nativeEvent: { layout: { x: 0, y: 0, width: WIDTH, height: 32 } } });
  return { view, onChange, onCommit };
}

describe('Scrubber', () => {
  beforeEach(() => jest.clearAllMocks());

  it('a tap sets the value where it lands', async () => {
    const { onCommit } = await renderScrubber();
    await act(async () => fireGestureHandler(getByGestureTestId('s-tap'), [{ state: State.BEGAN, x: 30 }, { state: State.ACTIVE, x: 30 }, { state: State.END, x: 30 }]));
    expect(onCommit).toHaveBeenCalledWith(15);
  });

  it('a drag follows the finger and commits when it lifts', async () => {
    const { onChange, onCommit } = await renderScrubber();
    await act(async () => fireGestureHandler(getByGestureTestId('s-pan'), [
      { state: State.BEGAN, x: 100 },
      { state: State.ACTIVE, x: 120 },
      { x: 160 },
      { state: State.END, x: 160 },
    ]));
    expect(onChange).toHaveBeenCalledWith(60);
    expect(onChange).toHaveBeenLastCalledWith(80);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith(80);
  });

  it('settles on the detent near it, with a tick', async () => {
    const { onChange } = await renderScrubber({ max: 2, step: 0.01, value: 0.5, detent: 1 });
    await act(async () => fireGestureHandler(getByGestureTestId('s-pan'), [
      { state: State.BEGAN, x: 50 },
      { state: State.ACTIVE, x: 102 },
      { state: State.END, x: 102 },
    ]));
    // 102 of 200 is 1.02: close enough to 100 % to land on it.
    expect(onChange).toHaveBeenLastCalledWith(1);
    expect(Haptics.selectionAsync).toHaveBeenCalledTimes(1);
  });

  it('disabled, a tap says why and nothing moves', async () => {
    const onDisabledPress = jest.fn();
    const { onChange, onCommit } = await renderScrubber({ disabled: true, onDisabledPress });
    await act(async () => fireGestureHandler(getByGestureTestId('s-tap'), [{ state: State.BEGAN, x: 30 }, { state: State.ACTIVE, x: 30 }, { state: State.END, x: 30 }]));
    expect(onDisabledPress).toHaveBeenCalled();
    expect(onChange).not.toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('a screen reader steps it, within its ends', async () => {
    const { view, onCommit } = await renderScrubber({ value: 95, accessibilityStep: 10 });
    const scrubber = view.getByLabelText('Volume');
    await fireEvent(scrubber, 'accessibilityAction', { nativeEvent: { actionName: 'increment' } });
    expect(onCommit).toHaveBeenLastCalledWith(100);
    await fireEvent(scrubber, 'accessibilityAction', { nativeEvent: { actionName: 'decrement' } });
    expect(onCommit).toHaveBeenLastCalledWith(85);
  });
});
