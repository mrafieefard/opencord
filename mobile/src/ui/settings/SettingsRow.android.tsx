// A row of a settings list, as in Android Settings: full width, 56 dp at
// least, the title with what is set under it in `textMuted` (Material's
// summary line), no chevrons. Level controls use `Scrubber`: Material's
// slider draws stop dots and a gap (GEMINI.md, the D80 review).
import { useRef } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';

import { Icon } from '../Icon';
import { Menu, type MenuHandle } from '../Menu';
import { Row } from '../Row';
import { Scrubber } from '../Scrubber';
import { Switch } from '../Switch';
import { Text } from '../Text';
import { space } from '../theme/space';
import { MAX_FONT_SCALE, typography } from '../theme/typography';
import { useTheme } from '../theme/useTheme';
import {
  choiceLabel,
  choiceMenuId,
  sliderStep,
  type ChoiceRowProps,
  type SettingsRowProps,
  type TextRowProps,
} from './SettingsRow.types';

export type { SettingsRowProps } from './SettingsRow.types';

/** Material's one-line list item. */
export const ROW_HEIGHT = 56;

export function SettingsRow(props: SettingsRowProps) {
  switch (props.kind) {
    case 'link':
      return (
        <Row accessibilityLabel={props.label} onPress={props.onPress} height={ROW_HEIGHT} style={styles.pad}>
          <View style={styles.inline}>
            {props.icon && <Icon name={props.icon} size={24} color="textSecondary" />}
            <Titles title={props.label} summary={props.value} />
          </View>
        </Row>
      );
    case 'switch':
      return (
        <View style={[styles.plain, styles.pad]}>
          <Titles title={props.label} summary={props.detail} />
          <Switch value={props.value} onChange={props.onChange} disabled={props.disabled} accessibilityLabel={props.label} />
        </View>
      );
    case 'slider':
      return (
        <View style={[styles.stacked, styles.pad]}>
          <View style={styles.line}>
            <Text variant="body" numberOfLines={1} style={styles.fill}>
              {props.label}
            </Text>
            <Text variant="callout" color="textMuted" tabular>
              {props.format(props.value)}
            </Text>
          </View>
          <Scrubber
            value={props.value}
            min={props.min}
            max={props.max}
            step={sliderStep(props)}
            onChange={props.onChange}
            onCommit={props.onDone}
            accessibilityLabel={props.label}
            accessibilityValue={props.format(props.value)}
          />
        </View>
      );
    case 'choice':
      return <ChoiceRow {...props} />;
    case 'check':
      return (
        <Row
          accessibilityLabel={props.label}
          accessibilityRole="radio"
          checked={props.checked}
          onPress={props.onPress}
          height={ROW_HEIGHT}
          style={styles.pad}
        >
          <View style={styles.inline}>
            <Titles title={props.label} summary={props.detail} />
            {/* Room for the mark on every row, so the labels' wrapping doesn't change with the choice. */}
            <View testID={props.checked ? 'settings-check' : undefined} style={styles.checkSlot}>
              {props.checked && <Icon name="check" size={24} color="text" />}
            </View>
          </View>
        </Row>
      );
    case 'text':
      return <TextRow {...props} />;
    case 'destructive':
      return (
        <Row accessibilityLabel={props.label} disabled={props.disabled} onPress={props.disabled ? undefined : props.onPress} height={ROW_HEIGHT} style={styles.pad}>
          <Text variant="body" color={props.disabled ? 'textMuted' : 'recording'} numberOfLines={1} style={styles.fill}>
            {props.label}
          </Text>
        </Row>
      );
    case 'action':
      return (
        <Row accessibilityLabel={props.label} disabled={props.disabled} onPress={props.disabled ? undefined : props.onPress} height={ROW_HEIGHT} style={styles.pad}>
          <Titles title={props.label} summary={props.detail} disabled={props.disabled} />
        </Row>
      );
    case 'value':
      return (
        <View accessible accessibilityLabel={`${props.label}, ${props.value}`} style={[styles.plain, styles.pad]}>
          <Titles title={props.label} summary={props.value} mono={props.mono} />
        </View>
      );
  }
}

/** The title, and under it the summary when there is one. */
function Titles({ title, summary, mono, disabled }: { title: string; summary?: string; mono?: boolean; disabled?: boolean }) {
  return (
    <View style={styles.fill}>
      <Text variant="body" color={disabled ? 'textMuted' : 'text'} numberOfLines={2}>
        {title}
      </Text>
      {summary !== undefined && (
        <Text variant={mono ? 'mono' : 'callout'} color="textMuted" numberOfLines={2}>
          {summary}
        </Text>
      )}
    </View>
  );
}

function ChoiceRow(props: ChoiceRowProps) {
  // The row takes the tap before the menu's own gesture sees it, so it
  // opens the menu itself (Menu.android.tsx).
  const menu = useRef<MenuHandle>(null);
  return (
    <Menu
      ref={menu}
      testID={choiceMenuId(props.label)}
      trigger="press"
      actions={props.options.map((option) => ({ id: option.key, title: option.label }))}
      onAction={props.onChange}
    >
      <Row accessibilityLabel={props.label} onPress={() => menu.current?.open()} height={ROW_HEIGHT} style={styles.pad}>
        <View style={styles.inline}>
          <Titles title={props.label} summary={choiceLabel(props)} />
        </View>
      </Row>
    </Menu>
  );
}

function TextRow(props: TextRowProps) {
  const { colors } = useTheme();
  return (
    <View style={[styles.stacked, styles.pad]}>
      <Text variant="footnote" color="textMuted">
        {props.label}
      </Text>
      <TextInput
        value={props.value}
        onChangeText={props.onChange}
        placeholder={props.placeholder}
        placeholderTextColor={colors.textMuted}
        maxLength={props.maxLength}
        multiline={props.multiline}
        maxFontSizeMultiplier={MAX_FONT_SCALE}
        accessibilityLabel={props.label}
        selectionColor={colors.text}
        cursorColor={colors.text}
        underlineColorAndroid="transparent"
        style={[
          typography.body,
          styles.field,
          { color: colors.text, borderBottomColor: colors.border },
          props.multiline && styles.multiline,
        ]}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  pad: { paddingHorizontal: space.lg },
  /** A row's content, side by side (Row's own style goes to its touchable). */
  inline: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: space.lg, paddingVertical: space.sm },
  plain: { minHeight: ROW_HEIGHT, flexDirection: 'row', alignItems: 'center', paddingVertical: space.sm, gap: space.lg },
  fill: { flex: 1 },
  checkSlot: { width: 24, alignItems: 'center' },
  stacked: { minHeight: ROW_HEIGHT, paddingVertical: space.md, gap: space.xs },
  line: { flexDirection: 'row', alignItems: 'center', gap: space.md },
  field: { paddingVertical: space.xs, paddingHorizontal: 0, borderBottomWidth: StyleSheet.hairlineWidth },
  multiline: { minHeight: 88, textAlignVertical: 'top' },
});
