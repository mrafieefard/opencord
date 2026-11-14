import { fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { ControlBar } from '../ControlBar';

async function bar(onOpenActivities?: () => void) {
  await render(
    <GestureHandlerRootView>
      <ControlBar
        muted={false}
        deafened={false}
        onToggleMute={jest.fn()}
        onToggleDeafen={jest.fn()}
        onOpenSoundboard={jest.fn()}
        onOpenActivities={onOpenActivities}
        onLeave={jest.fn()}
      />
    </GestureHandlerRootView>,
  );
}

describe('the control bar', () => {
  // Fails if the Activities button shows when the call can't start one, or
  // doesn't open the picker.
  it('the Activities button shows only when given, after the soundboard', async () => {
    await bar();
    expect(screen.queryByTestId('control-activities')).toBeNull();
    const open = jest.fn();
    await bar(open);
    const button = screen.getByTestId('control-activities');
    expect(button.props.accessibilityLabel).toBe('Activities');
    const order = screen.getAllByTestId(/^control-/).map((node) => node.props.testID as string);
    expect(order.indexOf('control-activities')).toBe(order.indexOf('control-soundboard') + 1);
    await fireEvent.press(button);
    expect(open).toHaveBeenCalledTimes(1);
  });
});
