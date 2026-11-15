// The voice call's controls on iOS (ControlBar.tsx is Android's): round
// Liquid Glass buttons, as the Phone app's call controls, in one frame (a
// softly filled capsule, not glass), all drawn in SwiftUI. Off, a control is glass faintly tinted
// with the text colour; Mute, Deafen and Activities are on (glass in the
// accent) when their state is; Leave is glass in the recording colour. Activities is on while an activity runs
// in the call: then it's a menu (open it, change it, close it), which grows
// out of that button alone; off, it opens the picker.
import { Button, HStack, Host, Image, Menu, Spacer } from '@expo/ui/swift-ui';
import { accessibilityLabel, background, buttonBorderShape, buttonStyle, controlSize, foregroundStyle, frame, padding, shapes, tint } from '@expo/ui/swift-ui/modifiers';
import { StyleSheet, View } from 'react-native';

import { onHue, space, useTheme, withAlpha, type IconName } from '@/ui';
import { icons } from '@/ui/icons';

import type { ControlBarProps } from './ControlBar';

export type { ControlBarProps } from './ControlBar';

/** The row's height, held from the first layout: SwiftUI measures it a moment later. */
const ROW_HEIGHT = 72;
/** The frame's room around its buttons. */
const FRAME_PAD = space.sm;
/** A control's glyph, and the square it's set in. */
const GLYPH = 20;
const GLYPH_BOX = 24;

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
    controlSize('large'),
    tint(tone === 'on' ? colors.accent : tone === 'leave' ? colors.recording : withAlpha(colors.text, 0.14)),
    foregroundStyle(tone === 'on' ? colors.onAccent : tone === 'leave' ? onHue : colors.text),
    accessibilityLabel(label),
  ];
  // Its glyph in a fixed square: a circle sizes to what it holds, so every
  // control is the same size whatever its symbol's shape.
  const glyph = (icon: IconName) => <Image systemName={icons[icon].ios} size={GLYPH} modifiers={[frame({ width: GLYPH_BOX, height: GLYPH_BOX })]} />;
  const control = (icon: IconName, label: string, tone: Tone, testID: string, onPress?: () => void) => (
    <Button onPress={onPress} modifiers={look(label, tone)} testID={testID}>
      {glyph(icon)}
    </Button>
  );
  const showActivities = onOpenActivities !== undefined || activityActive;
  const withMenu = activityActive && activityMenu.length > 0 && onActivityAction !== undefined;

  return (
    <View style={styles.row} testID="control-bar">
      <Host matchContents={{ vertical: true }} colorScheme={scheme} style={styles.host}>
        {/* The frame: one capsule the buttons sit in. */}
        <HStack
          modifiers={[
            padding({ vertical: FRAME_PAD, horizontal: FRAME_PAD }),
            // A soft fill, not glass: a frame of glass would be what the
            // Activities menu grows out of, taking every button with it.
            background(withAlpha(colors.text, 0.07), shapes.capsule()),
          ]}
        >
          <Spacer />
          {control(muted ? 'micOff' : 'mic', muted ? 'Unmute' : 'Mute', muted ? 'on' : 'plain', 'control-mute', onToggleMute)}
          <Spacer />
          {control(deafened ? 'deafen' : 'listen', deafened ? 'Undeafen' : 'Deafen', deafened ? 'on' : 'plain', 'control-deafen', onToggleDeafen)}
          <Spacer />
          {control('soundboard', 'Soundboard', 'plain', 'control-soundboard', onOpenSoundboard)}
          {showActivities && <Spacer />}
          {showActivities &&
            (withMenu ? (
              <Menu label={glyph('extension')} modifiers={look('Activities', 'on')} testID="activities-menu">
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
  row: { marginHorizontal: space.md, marginBottom: space.sm },
  host: { minHeight: ROW_HEIGHT },
});
