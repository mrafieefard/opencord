import { fireEvent, render, screen } from '@testing-library/react-native';

import { Button } from '../Button.ios';

describe('Button on iOS', () => {
  it('acts on a press', async () => {
    const onPress = jest.fn();
    await render(<Button title="Join voice" variant="voice" onPress={onPress} />);
    await fireEvent.press(screen.getByRole('button', { name: 'Join voice' }));
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it('ignores presses while loading, and is off when disabled', async () => {
    const onPress = jest.fn();
    await render(<Button title="Add" variant="primary" loading onPress={onPress} />);
    await fireEvent.press(screen.getByRole('button', { name: 'Add' }));
    expect(onPress).not.toHaveBeenCalled();

    await render(<Button title="This channel is full" variant="voice" disabled onPress={onPress} />);
    expect(screen.getByRole('button', { name: 'This channel is full' })).toBeDisabled();
  });
});
