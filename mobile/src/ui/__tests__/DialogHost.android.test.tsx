import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { confirm } from '../confirm.android';
import { DialogHost } from '../DialogHost.android';

async function renderHost() {
  await render(
    <GestureHandlerRootView>
      <DialogHost />
    </GestureHandlerRootView>,
  );
}

async function ask(onConfirm: () => void) {
  await act(async () => {
    confirm({
      title: 'Leave Night Owls?',
      message: 'You can come back with an invite.',
      confirmTitle: 'Leave',
      destructive: true,
      onConfirm,
    });
  });
}

describe('the Android dialog', () => {
  it('shows the question, and Cancel closes it without acting', async () => {
    const onConfirm = jest.fn();
    await renderHost();
    expect(screen.queryByText('Leave Night Owls?')).toBeNull();

    await ask(onConfirm);
    expect(screen.getByText('Leave Night Owls?')).toBeTruthy();
    expect(screen.getByText('You can come back with an invite.')).toBeTruthy();

    await fireEvent.press(screen.getByLabelText('Cancel'));
    expect(screen.queryByText('Leave Night Owls?')).toBeNull();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('confirming closes it and acts once', async () => {
    const onConfirm = jest.fn();
    await renderHost();

    await ask(onConfirm);
    await fireEvent.press(screen.getByLabelText('Leave'));
    expect(screen.queryByText('Leave Night Owls?')).toBeNull();
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
