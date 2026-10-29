import { fireEvent, render, screen } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { Avatar, initials, nameHash } from '../Avatar';
import { Badge, badgeText } from '../Badge';
import { Button } from '../Button';
import { PaneHeader } from '../PaneHeader';
import { Text } from '../Text';
import { onHue, tokens } from '../theme/tokens';
import { typography, type TextVariant } from '../theme/typography';

describe('Text', () => {
  it.each(Object.keys(typography) as TextVariant[])('%s has its size, line height and family', async (variant) => {
    await render(<Text variant={variant}>words</Text>);
    const style = StyleSheet.flatten(screen.getByText('words').props.style);
    expect(style).toMatchObject(typography[variant]);
  });
});

describe('Avatar', () => {
  it('initials come from the first two words', () => {
    expect(initials('Kai Nakamura Jr')).toBe('KN');
    expect(initials('mira')).toBe('M');
    expect(initials('  Tomás  Ruiz ')).toBe('TR');
  });

  it('the shade is the same for the same name', () => {
    expect(nameHash('Kai')).toBe(nameHash('Kai'));
    expect(nameHash('Kai') % 5).toBe(nameHash('Kai') % 5);
  });

  it('a speaking ring shows only while speaking', async () => {
    const { rerender } = await render(<Avatar name="Kai" size={22} />);
    expect(StyleSheet.flatten(screen.getByTestId('avatar-ring').props.style).opacity).toBe(0);
    await rerender(<Avatar name="Kai" size={22} speaking />);
    const ring = StyleSheet.flatten(screen.getByTestId('avatar-ring').props.style);
    expect(ring.opacity).toBe(1);
    expect(ring.borderColor).toBe(tokens.dark.voice);
    expect(ring.borderWidth).toBe(2);
  });
});

describe('Badge', () => {
  it('nothing at zero', async () => {
    expect(badgeText(0)).toBeNull();
    await render(<Badge count={0} />);
    expect(screen.toJSON()).toBeNull();
  });

  it('99+ above 99', () => {
    expect(badgeText(100)).toBe('99+');
    expect(badgeText(99)).toBe('99');
  });

  it('mentions start with @', () => {
    expect(badgeText(3, true)).toBe('@3');
  });
});

describe('PaneHeader', () => {
  it('actions have their labels and call back', async () => {
    const back = jest.fn();
    const more = jest.fn();
    await render(
      <PaneHeader
        title="general"
        subtitle="The usual chatter"
        left={{ icon: 'back', label: 'Servers', onPress: back }}
        right={[{ icon: 'more', label: 'More', onPress: more }]}
      />,
    );
    expect(screen.getByText('general')).toBeTruthy();
    expect(screen.getByText('The usual chatter')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Servers'));
    await fireEvent.press(screen.getByLabelText('More'));
    expect(back).toHaveBeenCalledTimes(1);
    expect(more).toHaveBeenCalledTimes(1);
  });
});

describe('Button', () => {
  it('on the hues the words are white, in the dark theme too', async () => {
    await render(<Button title="Join voice" variant="voice" onPress={() => {}} />);
    expect(onHue).toBe('#FFFFFF');
    expect(StyleSheet.flatten(screen.getByText('Join voice').props.style).color).toBe(onHue);
    await render(<Button title="Leave" variant="danger" onPress={() => {}} />);
    expect(StyleSheet.flatten(screen.getByText('Leave').props.style).color).toBe(onHue);
  });

  it('disabled, the words are muted whatever the variant', async () => {
    await render(<Button title="This channel is full" variant="voice" disabled onPress={() => {}} />);
    expect(StyleSheet.flatten(screen.getByText('This channel is full').props.style).color).toBe(tokens.dark.textMuted);
  });
});
