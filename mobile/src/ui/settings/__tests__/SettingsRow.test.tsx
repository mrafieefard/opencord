import { act, fireEvent, render, screen } from '@testing-library/react-native';
import type { ReactElement } from 'react';
import { StyleSheet } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { setErrorShower } from '@/core/report';

import { tokens } from '../../theme/tokens';
import { SettingsList } from '../SettingsList';
import { SettingsRow as AndroidRow } from '../SettingsRow.android';
import { SettingsRow as IosRow } from '../SettingsRow.ios';
import { SettingsSection } from '../SettingsSection';
import { useSaveButton } from '../useSaveButton';

jest.mock('expo-haptics', () => ({ selectionAsync: jest.fn(async () => {}) }));

// One navigation object, so a test reads the options the page last set.
const mockNavigation = { setOptions: jest.fn(), addListener: jest.fn(() => () => {}) };
jest.mock('expo-router', () => ({ useNavigation: () => mockNavigation }));

// Jest draws with iOS's files (the preset's default platform), so both legs
// use iOS's Switch, Slider and Menu inside; each leg checks its own row's
// layout and its own level control (SwiftUI's slider on iOS, the Scrubber
// on Android). Android's own controls are looked at on the emulator.
const ROWS = [
  ['iOS', IosRow],
  ['Android', AndroidRow],
] as const;

async function draw(element: ReactElement) {
  const view = await render(<GestureHandlerRootView>{element}</GestureHandlerRootView>);
  return { redraw: (next: ReactElement) => view.rerender(<GestureHandlerRootView>{next}</GestureHandlerRootView>) };
}

/** Lets every settled promise's callbacks run. */
const flush = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

describe.each(ROWS)('SettingsRow on %s', (system, SettingsRow) => {
  it('a link row calls onPress', async () => {
    const onPress = jest.fn();
    await draw(<SettingsRow kind="link" label="Roles" value="3" onPress={onPress} />);
    expect(screen.getByText('3')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Roles'));
    await fireEvent.press(screen.getByLabelText('Roles'));
    expect(onPress).toHaveBeenCalledTimes(2);
  });

  it('a switch row shows its value and calls onChange', async () => {
    const onChange = jest.fn();
    const row = (value: boolean) => (
      <SettingsRow kind="switch" label="Show join messages" detail="In the system channel" value={value} onChange={onChange} />
    );
    const { redraw } = await draw(row(false));
    expect(screen.getByText('In the system channel')).toBeTruthy();
    const off = screen.getByRole('switch', { name: 'Show join messages' });
    expect(off.props.accessibilityState).toMatchObject({ checked: false });
    await fireEvent.press(off);
    expect(onChange).toHaveBeenLastCalledWith(true);

    // The page stores the new value and draws the row again.
    await redraw(row(true));
    const on = screen.getByRole('switch', { name: 'Show join messages' });
    expect(on.props.accessibilityState).toMatchObject({ checked: true });
    await fireEvent.press(on);
    expect(onChange).toHaveBeenLastCalledWith(false);
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it('a choice row opens the menu with each option', async () => {
    const onChange = jest.fn();
    const options = [
      { key: 'everyone', label: 'Everyone' },
      { key: 'friends', label: 'Friends only' },
      { key: 'nobody', label: 'Nobody' },
    ];
    await draw(<SettingsRow kind="choice" label="Who can message you" value="friends" options={options} onChange={onChange} />);
    // The chosen option's words, not its key.
    expect(screen.getByText('Friends only')).toBeTruthy();
    // The system menu's items (jest draws iOS's UIMenu), answered by key, as
    // MusicPlayer.test.tsx answers its menu.
    const menu = screen.getByTestId('settings-choice-Who can message you');
    expect(menu.props.actions.map((action: { id: string; title: string }) => [action.id, action.title])).toEqual([
      ['everyone', 'Everyone'],
      ['friends', 'Friends only'],
      ['nobody', 'Nobody'],
    ]);
    await fireEvent(menu, 'action', 'nobody');
    await fireEvent(menu, 'action', 'everyone');
    expect(onChange.mock.calls).toEqual([['nobody'], ['everyone']]);
  });

  it('a destructive row is drawn in the danger colour', async () => {
    const onPress = jest.fn();
    await draw(<SettingsRow kind="destructive" label="Delete server" onPress={onPress} />);
    expect(StyleSheet.flatten(screen.getByText('Delete server').props.style).color).toBe(tokens.dark.recording);
    await fireEvent.press(screen.getByLabelText('Delete server'));
    await fireEvent.press(screen.getByLabelText('Delete server'));
    expect(onPress).toHaveBeenCalledTimes(2);
  });

  // Fails if an action row draws a chevron or doesn't call onPress each time.
  it('an action row acts at once, without a chevron', async () => {
    const onPress = jest.fn();
    await draw(<SettingsRow kind="action" label="Mark as read" onPress={onPress} />);
    expect(screen.queryByTestId('settings-chevron')).toBeNull();
    expect(StyleSheet.flatten(screen.getByText('Mark as read').props.style).color).toBe(tokens.dark.text);
    await fireEvent.press(screen.getByLabelText('Mark as read'));
    await fireEvent.press(screen.getByLabelText('Mark as read'));
    expect(onPress).toHaveBeenCalledTimes(2);
  });

  // Fails if a value row becomes pressable or hides its value.
  it('a value row shows its value and is not a button', async () => {
    await draw(<SettingsRow kind="value" label="Hosting" value="Open on a computer" />);
    expect(screen.getByText('Open on a computer')).toBeTruthy();
    expect(StyleSheet.flatten(screen.getByText('Open on a computer').props.style).color).toBe(tokens.dark.textMuted);
    expect(screen.getByLabelText('Hosting, Open on a computer').props.accessibilityRole).toBeUndefined();
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.queryByTestId('settings-chevron')).toBeNull();
  });

  it('a text row edits its value within its limit', async () => {
    const onChange = jest.fn();
    await draw(<SettingsRow kind="text" label="Server name" value="Night Owls" placeholder="Name" maxLength={100} onChange={onChange} />);
    const field = screen.getByLabelText('Server name');
    expect(field.props.value).toBe('Night Owls');
    expect(field.props.maxLength).toBe(100);
    await fireEvent.changeText(field, 'Night Owl');
    await fireEvent.changeText(field, 'Night Owls!');
    expect(onChange.mock.calls).toEqual([['Night Owl'], ['Night Owls!']]);
  });

  it('a slider row shows its formatted value, follows the finger and is done when it lifts', async () => {
    const onChange = jest.fn();
    const onDone = jest.fn();
    await draw(
      <SettingsRow
        kind="slider"
        label="Upload limit"
        value={25}
        min={1}
        max={100}
        step={1}
        format={(v) => `${v} MB`}
        onChange={onChange}
        onDone={onDone}
      />,
    );
    expect(screen.getByText('25 MB')).toBeTruthy();
    const slider = screen.getByLabelText('Upload limit');
    expect(slider.props.accessibilityRole).toBe('adjustable');
    if (system === 'iOS') {
      // SwiftUI's slider (the mock moves, then lifts).
      await fireEvent(slider, 'change', 30);
      await fireEvent(slider, 'change', 40);
      expect(onChange.mock.calls).toEqual([[30], [40]]);
      expect(onDone.mock.calls).toEqual([[30], [40]]);
    } else {
      // The Scrubber, stepped by a screen reader: one step from the value shown.
      expect(slider.props.accessibilityValue).toEqual({ text: '25 MB' });
      await fireEvent(slider, 'accessibilityAction', { nativeEvent: { actionName: 'increment' } });
      await fireEvent(slider, 'accessibilityAction', { nativeEvent: { actionName: 'decrement' } });
      expect(onChange.mock.calls).toEqual([[26], [24]]);
      expect(onDone.mock.calls).toEqual([[26], [24]]);
    }
  });

  it('chevrons only on iOS link rows, and value text in textMuted', async () => {
    await draw(
      <>
        <SettingsRow kind="link" label="Roles" value="3" onPress={jest.fn()} />
        <SettingsRow kind="switch" label="Show join messages" value onChange={jest.fn()} />
        <SettingsRow kind="choice" label="Theme" value="dark" options={[{ key: 'dark', label: 'Dark' }]} onChange={jest.fn()} />
      </>,
    );
    expect(StyleSheet.flatten(screen.getByText('3').props.style).color).toBe(tokens.dark.textMuted);
    expect(StyleSheet.flatten(screen.getByText('Dark').props.style).color).toBe(tokens.dark.textMuted);
    expect(screen.queryAllByTestId('settings-chevron')).toHaveLength(system === 'iOS' ? 1 : 0);
  });
});

describe('SettingsList', () => {
  it('draws the grouped background, chat, under surface cards', async () => {
    await draw(
      <SettingsList>
        <SettingsSection title="Profile">
          <IosRow kind="link" label="Roles" onPress={jest.fn()} />
        </SettingsSection>
      </SettingsList>,
    );
    expect(StyleSheet.flatten(screen.getByTestId('settings-list').props.style).backgroundColor).toBe(tokens.dark.chat);
    expect(tokens.dark.chat).not.toBe(tokens.dark.surface);
    expect(tokens.light.chat).not.toBe(tokens.light.surface);
  });
});

describe('SettingsSection', () => {
  it('shows its title and footer', async () => {
    await draw(
      <SettingsSection title="Profile" footer="Everyone on the server sees this.">
        <IosRow kind="link" label="Roles" onPress={jest.fn()} />
      </SettingsSection>,
    );
    expect(screen.getByText('Profile')).toBeTruthy();
    expect(screen.getByText('Everyone on the server sees this.')).toBeTruthy();
  });
});

describe('useSaveButton', () => {
  function SavePage(props: { dirty: boolean; saving: boolean; onSave: () => Promise<void> }) {
    useSaveButton(props);
    return null;
  }

  beforeEach(() => mockNavigation.setOptions.mockClear());

  /** The header's right side, as the navigator draws it from the last options. */
  async function header() {
    const options: { headerRight: () => ReactElement } = mockNavigation.setOptions.mock.calls.at(-1)?.[0];
    await render(<GestureHandlerRootView>{options.headerRight()}</GestureHandlerRootView>);
  }

  it('save is disabled until something changed', async () => {
    const onSave = jest.fn(async () => {});
    await render(<SavePage dirty={false} saving={false} onSave={onSave} />);
    await header();
    const clean = screen.getByRole('button', { name: 'Save' });
    expect(clean.props.accessibilityState).toMatchObject({ disabled: true });
    await fireEvent.press(clean);
    await fireEvent.press(clean);
    expect(onSave).not.toHaveBeenCalled();

    await render(<SavePage dirty saving={false} onSave={onSave} />);
    await header();
    const dirty = screen.getByRole('button', { name: 'Save' });
    expect(dirty.props.accessibilityState).toMatchObject({ disabled: false });
    await fireEvent.press(dirty);
    await flush();
    await fireEvent.press(dirty);
    await flush();
    expect(onSave).toHaveBeenCalledTimes(2);
  });

  it('a second press while the first save runs does nothing', async () => {
    let finish = () => {};
    const onSave = jest.fn(() => new Promise<void>((resolve) => (finish = () => resolve())));
    await render(<SavePage dirty saving={false} onSave={onSave} />);
    await header();
    await fireEvent.press(screen.getByRole('button', { name: 'Save' }));
    await fireEvent.press(screen.getByRole('button', { name: 'Save' }));
    expect(onSave).toHaveBeenCalledTimes(1);
    await act(async () => finish());
    await flush();
    // Once it is done, Save saves again.
    await fireEvent.press(screen.getByRole('button', { name: 'Save' }));
    expect(onSave).toHaveBeenCalledTimes(2);
    await act(async () => finish());
  });

  it('a failed save is reported, and Save works again', async () => {
    const shown: string[] = [];
    setErrorShower((text) => shown.push(text));
    const warn = jest.spyOn(console, 'warn').mockImplementation(() => {});
    const onSave = jest.fn(async () => {
      throw new Error('no connection');
    });
    await render(<SavePage dirty saving={false} onSave={onSave} />);
    await header();
    await fireEvent.press(screen.getByRole('button', { name: 'Save' }));
    await flush();
    expect(shown).toHaveLength(1);
    await fireEvent.press(screen.getByRole('button', { name: 'Save' }));
    await flush();
    expect(onSave).toHaveBeenCalledTimes(2);
    expect(shown).toHaveLength(2);
    warn.mockRestore();
    setErrorShower(() => {});
  });

  it('shows a spinner while saving, and save stays disabled', async () => {
    const onSave = jest.fn(async () => {});
    await render(<SavePage dirty saving onSave={onSave} />);
    await header();
    const saving = screen.getByRole('button', { name: 'Save' });
    expect(saving.props.accessibilityState).toMatchObject({ disabled: true, busy: true });
    expect(screen.getByTestId('save-spinner')).toBeTruthy();
    await fireEvent.press(saving);
    expect(onSave).not.toHaveBeenCalled();
  });
});
