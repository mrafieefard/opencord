// The voice call's controls (spec §8.6): 56 pt round buttons (Mute,
// Deafen, Soundboard, Activities, Leave) in one raised bar (controlLook).
// iOS draws its own, each its own Liquid Glass button (ControlBar.ios.tsx).
// Music and, on iOS, the sound output have their buttons in the header.
//
// Mute, Deafen and Activities are on (the accent) when their state is, and
// their glyph shows it; Leave is the recording colour with a white glyph.
// Activities is on while an activity runs in the call: pressed then, it
// offers a menu (open it, change it, close it); off, it opens the picker.
import { useCallback, useRef, type ReactNode } from 'react';
import { Touchable } from 'react-native-gesture-handler';

import { Icon, Menu, onHue, type IconName, type MenuAction, type MenuHandle } from '@/ui';

import { ControlCircle, ControlFrame, type ControlTone } from './controlLook';

export interface ControlBarProps {
  muted: boolean;
  deafened: boolean;
  onToggleMute: () => void;
  onToggleDeafen: () => void;
  onOpenSoundboard?: () => void;
  /** Given when this call can start an activity (plugins M4 Task 14): the picker. */
  onOpenActivities?: () => void;
  /** An activity runs in the call: the button is on, and offers `activityMenu`. */
  activityActive?: boolean;
  activityMenu?: MenuAction[];
  onActivityAction?: (id: string) => void;
  onLeave: () => void;
}

interface ControlProps {
  icon: IconName;
  label: string;
  tone?: ControlTone;
  /** A toggle's state, for VoiceOver and TalkBack. */
  selected?: boolean;
  onPress?: () => void;
  testID: string;
}

/** One round control: its glass (iOS) or fill (Android), and its glyph. */
function Control({ icon, label, tone = 'plain', selected, onPress, testID }: ControlProps) {
  return (
    <Touchable
      onPress={onPress}
      accessible
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={selected === undefined ? undefined : { selected }}
      testID={testID}
    >
      <ControlCircle tone={tone}>
        {tone === 'leave' ? <Icon name={icon} size={24} tint={onHue} /> : <Icon name={icon} size={24} color={tone === 'on' ? 'onAccent' : 'text'} />}
      </ControlCircle>
    </Touchable>
  );
}

interface ActivitiesButtonProps {
  active: boolean;
  menu: MenuAction[];
  onAction?: (id: string) => void;
  onOpenPicker?: () => void;
}

/** Activities: off, the picker; on (one running), its menu, from this button alone. */
function ActivitiesButton({ active, menu: actions, onAction, onOpenPicker }: ActivitiesButtonProps) {
  const menu = useRef<MenuHandle>(null);
  const withMenu = active && actions.length > 0 && onAction !== undefined;
  const press = useCallback(() => {
    if (withMenu) menu.current?.open();
    else onOpenPicker?.();
  }, [withMenu, onOpenPicker]);
  const button: ReactNode = (
    <Control icon="extension" label="Activities" tone={active ? 'on' : 'plain'} selected={active} onPress={press} testID="control-activities" />
  );
  return withMenu ? (
    <Menu ref={menu} actions={actions} onAction={onAction} trigger="press" testID="activities-menu">
      {button}
    </Menu>
  ) : (
    button
  );
}

export function ControlBar({
  muted,
  deafened,
  onToggleMute,
  onToggleDeafen,
  onOpenSoundboard,
  onOpenActivities,
  activityActive = false,
  activityMenu = [],
  onActivityAction,
  onLeave,
}: ControlBarProps) {
  return (
    <ControlFrame>
      <Control
        icon={muted ? 'micOff' : 'mic'}
        label={muted ? 'Unmute' : 'Mute'}
        tone={muted ? 'on' : 'plain'}
        onPress={onToggleMute}
        testID="control-mute"
      />
      <Control
        icon={deafened ? 'deafen' : 'listen'}
        label={deafened ? 'Undeafen' : 'Deafen'}
        tone={deafened ? 'on' : 'plain'}
        onPress={onToggleDeafen}
        testID="control-deafen"
      />
      <Control icon="soundboard" label="Soundboard" onPress={onOpenSoundboard} testID="control-soundboard" />
      {(onOpenActivities || activityActive) && (
        <ActivitiesButton active={activityActive} menu={activityMenu} onAction={onActivityAction} onOpenPicker={onOpenActivities} />
      )}
      <Control icon="leave" label="Leave voice" tone="leave" onPress={onLeave} testID="control-leave" />
    </ControlFrame>
  );
}
