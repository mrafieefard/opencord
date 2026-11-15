// The voice call's controls on iOS (ControlBar.tsx is Android's): a row of
// round Liquid Glass buttons, as the Phone app's call controls, each its
// own glass with no bar around them. Off, a control is glass faintly tinted
// with the text colour; Mute, Deafen and Activities are on (glass in the
// accent) when their state is; Leave is glass in the recording colour. Activities is on while an activity runs
// in the call: then it's a menu (open it, change it, close it), which grows
// out of that button alone; off, it opens the picker.
import { Button, HStack, Host, Menu, Spacer } from '@expo/ui/swift-ui';
import { accessibilityLabel, buttonBorderShape, buttonStyle, controlSize, foregroundStyle, labelStyle, tint } from '@expo/ui/swift-ui/modifiers';
import { StyleSheet, View } from 'react-native';

import { onHue, space, useTheme, withAlpha, type IconName } from '@/ui';
import { icons } from '@/ui/icons';

import type { ControlBarProps } from './ControlBar';

export type { ControlBarProps } from './ControlBar';

/** The row's height, held from the first layout: SwiftUI measures it a moment later. */
const ROW_HEIGHT = 72;

type Tone = 'plain' | 'on' | 'leave';

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
  const { colors, scheme } = useTheme();
  // Prominent glass for every control: plain glass vanishes over the call's
  // flat colour (and a sheet's glass), so an off control is the glass faintly
  // tinted with the text colour, and its glyph is set for either theme.
  const look = (label: string, tone: Tone) => [
    buttonStyle('glassProminent'),
    buttonBorderShape('circle'),
    controlSize('extraLarge'),
    labelStyle('iconOnly'),
    tint(tone === 'on' ? colors.accent : tone === 'leave' ? colors.recording : withAlpha(colors.text, 0.14)),
    foregroundStyle(tone === 'on' ? colors.onAccent : tone === 'leave' ? onHue : colors.text),
    accessibilityLabel(label),
  ];
  const control = (icon: IconName, label: string, tone: Tone, testID: string, onPress?: () => void) => (
    <Button label={label} systemImage={icons[icon].ios} onPress={onPress} modifiers={look(label, tone)} testID={testID} />
  );
  const showActivities = onOpenActivities !== undefined || activityActive;
  const withMenu = activityActive && activityMenu.length > 0 && onActivityAction !== undefined;

  return (
    <View style={styles.row} testID="control-bar">
      <Host matchContents={{ vertical: true }} colorScheme={scheme} style={styles.host}>
        <HStack>
          <Spacer />
          {control(muted ? 'micOff' : 'mic', muted ? 'Unmute' : 'Mute', muted ? 'on' : 'plain', 'control-mute', onToggleMute)}
          <Spacer />
          {control(deafened ? 'deafen' : 'listen', deafened ? 'Undeafen' : 'Deafen', deafened ? 'on' : 'plain', 'control-deafen', onToggleDeafen)}
          <Spacer />
          {control('soundboard', 'Soundboard', 'plain', 'control-soundboard', onOpenSoundboard)}
          {showActivities && <Spacer />}
          {showActivities &&
            (withMenu ? (
              <Menu label="Activities" systemImage={icons.extension.ios} modifiers={look('Activities', 'on')} testID="activities-menu">
                {activityMenu.map((action) => (
                  <Button
                    key={action.id}
                    label={action.title}
                    role={action.destructive ? 'destructive' : 'default'}
                    onPress={() => onActivityAction(action.id)}
                  />
                ))}
              </Menu>
            ) : (
              control('extension', 'Activities', activityActive ? 'on' : 'plain', 'control-activities', onOpenActivities)
            ))}
          <Spacer />
          {control('leave', 'Leave voice', 'leave', 'control-leave', onLeave)}
          <Spacer />
        </HStack>
      </Host>
    </View>
  );
}

const styles = StyleSheet.create({
  row: { marginHorizontal: space.sm, marginBottom: space.sm },
  host: { minHeight: ROW_HEIGHT },
});
