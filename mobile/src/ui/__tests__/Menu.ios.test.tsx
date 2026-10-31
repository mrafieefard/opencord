import { fireEvent, render, screen, within } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { Menu, type MenuAction } from '../Menu.ios';
import { Row } from '../Row';
import { Text } from '../Text';

const ACTIONS: MenuAction[] = [
  { id: 'read', title: 'Mark as read', icon: 'check' },
  { id: 'retry', title: 'Retry now', icon: 'retry', disabled: true },
  { id: 'leave', title: 'Leave server', icon: 'leaveServer', destructive: true },
];

async function renderRow(onPress: () => void, onAction: (id: string) => void, actions = ACTIONS) {
  await render(
    <GestureHandlerRootView>
      <Menu testID="menu" trigger="longPress" actions={actions} onAction={onAction}>
        <Row accessibilityLabel="Night Owls" onPress={onPress}>
          <Text variant="headline">Night Owls</Text>
        </Row>
      </Menu>
    </GestureHandlerRootView>,
  );
}

async function layOut() {
  await fireEvent(screen.getByTestId('menu'), 'layout', { nativeEvent: { layout: { x: 0, y: 0, width: 390, height: 72 } } });
}

describe('Menu on iOS, on a long press', () => {
  it('lets a tap through to the row, before and after the row is measured', async () => {
    const onPress = jest.fn();
    await renderRow(onPress, jest.fn());
    await fireEvent.press(screen.getByLabelText('Night Owls'));
    await layOut();
    await fireEvent.press(screen.getByLabelText('Night Owls'));
    expect(onPress).toHaveBeenCalledTimes(2);
  });

  it('offers each action once the row is measured, and answers with its id', async () => {
    const onAction = jest.fn();
    await renderRow(jest.fn(), onAction);
    expect(screen.queryAllByRole('menuitem')).toHaveLength(0);
    await layOut();
    const items = within(screen.getByTestId('menu')).getAllByRole('menuitem');
    expect(items.map((item) => item.props.accessibilityLabel)).toEqual(['Mark as read', 'Retry now', 'Leave server']);
    await fireEvent.press(screen.getByLabelText('Leave server'));
    await fireEvent.press(screen.getByLabelText('Retry now'));
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onAction).toHaveBeenCalledWith('leave');
  });

  it('is only its content when it has no actions', async () => {
    const onPress = jest.fn();
    await renderRow(onPress, jest.fn(), []);
    expect(screen.queryByTestId('menu')).toBeNull();
    await fireEvent.press(screen.getByLabelText('Night Owls'));
    expect(onPress).toHaveBeenCalledTimes(1);
  });
});
