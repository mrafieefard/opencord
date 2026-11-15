// A row of an inset grouped list, as in iOS Settings: 44 pt, the label on
// the left, what is set on the right in `textMuted`, a chevron only on rows
// that open another page. The group around it (SettingsSection) draws the
// card and the separators.
import { useRef } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';

import { Icon } from '../Icon';
import { Menu, type MenuHandle } from '../Menu';
import { Row } from '../Row';
import { Slider } from '../Slider';
import { Switch } from '../Switch';
import { Text } from '../Text';
import { space, TOUCH } from '../theme/space';
import { MAX_FONT_SCALE, typography } from '../theme/typography';
import { useTheme } from '../theme/useTheme';
import {
  choiceLabel,
  choiceMenuId,
  sliderStep,
  type ChoiceRowProps,
  type SettingsRowProps,
  type SliderRowProps,
  type TextRowProps,
} from './SettingsRow.types';

export type { SettingsRowProps } from './SettingsRow.types';

/** iOS's row height. */
export const ROW_HEIGHT = TOUCH;

export function SettingsRow(props: SettingsRowProps) {
  switch (props.kind) {
    case 'link':
      return (
        <Row accessibilityLabel={props.label} onPress={props.onPress} height={ROW_HEIGHT} style={styles.pad}>
          <View style={styles.inline}>
            {props.icon && <Icon name={props.icon} size={20} color="textSecondary" />}
            <Text variant="body" numberOfLines={1} style={styles.label}>
              {props.label}
            </Text>
            {props.value !== undefined && (
              <Text variant="body" color="textMuted" numberOfLines={1} style={styles.value}>
                {props.value}
              </Text>
            )}
            {/* chevron.left turned round, as on the You tab. */}
            <View testID="settings-chevron" style={styles.chevron}>
              <Icon name="back" size={14} color="textMuted" />
            </View>
          </View>
        </Row>
      );
    case 'switch':
      return (
        <View style={[styles.plain, styles.pad]}>
          <View style={styles.label}>
            <Text variant="body">{props.label}</Text>
            {props.detail !== undefined && (
              <Text variant="footnote" color="textMuted">
                {props.detail}
              </Text>
            )}
          </View>
          <Switch value={props.value} onChange={props.onChange} disabled={props.disabled} accessibilityLabel={props.label} />
        </View>
      );
    case 'slider':
      return <SliderRow {...props} />;
    case 'choice':
      return <ChoiceRow {...props} />;
    case 'text':
      return <TextRow {...props} />;
    case 'destructive':
      return (
        <Row accessibilityLabel={props.label} onPress={props.onPress} height={ROW_HEIGHT} style={styles.pad}>
          <Text variant="body" color="recording" numberOfLines={1} style={styles.label}>
            {props.label}
          </Text>
        </Row>
      );
    case 'action':
      return (
        <Row accessibilityLabel={props.label} onPress={props.onPress} height={ROW_HEIGHT} style={styles.pad}>
          <Text variant="body" numberOfLines={1} style={styles.label}>
            {props.label}
          </Text>
        </Row>
      );
    case 'value':
      return (
        <View accessible accessibilityLabel={`${props.label}, ${props.value}`} style={[styles.plain, styles.pad]}>
          <Text variant="body" numberOfLines={1} style={styles.label}>
            {props.label}
          </Text>
          <Text variant="body" color="textMuted" numberOfLines={1} style={styles.value}>
            {props.value}
          </Text>
        </View>
      );
  }
}

function SliderRow(props: SliderRowProps) {
  return (
    <View style={[styles.slider, styles.pad]}>
      <View style={styles.line}>
        <Text variant="body" numberOfLines={1} style={styles.label}>
          {props.label}
        </Text>
        <Text variant="body" color="textMuted" tabular>
          {props.format(props.value)}
        </Text>
      </View>
      <View style={styles.line}>
        <Slider
          value={props.value}
          min={props.min}
          max={props.max}
          step={sliderStep(props)}
          onChange={props.onChange}
          onCommit={props.onDone}
          accessibilityLabel={props.label}
        />
      </View>
    </View>
  );
}

function ChoiceRow(props: ChoiceRowProps) {
  // iOS's menu opens from its own press; `open()` serves Android's.
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
          <Text variant="body" numberOfLines={1} style={styles.label}>
            {props.label}
          </Text>
          <Text variant="body" color="textMuted" numberOfLines={1} style={styles.value}>
            {choiceLabel(props)}
          </Text>
        </View>
      </Row>
    </Menu>
  );
}

function TextRow(props: TextRowProps) {
  const { colors } = useTheme();
  const field = (
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
      style={[
        typography.body,
        { color: colors.text },
        props.multiline ? styles.multiline : styles.singleLine,
      ]}
    />
  );
  if (props.multiline) {
    return (
      <View style={[styles.stacked, styles.pad]}>
        <Text variant="footnote" color="textMuted">
          {props.label}
        </Text>
        {field}
      </View>
    );
  }
  return (
    <View style={[styles.plain, styles.pad]}>
      <Text variant="body" numberOfLines={1}>
        {props.label}
      </Text>
      {field}
    </View>
  );
}

const styles = StyleSheet.create({
  pad: { paddingHorizontal: space.lg },
  /** A row's content, side by side (Row's own style goes to its touchable). */
  inline: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: space.md },
  plain: { minHeight: ROW_HEIGHT, flexDirection: 'row', alignItems: 'center', paddingVertical: space.sm, gap: space.md },
  label: { flex: 1 },
  // The label keeps at least half the row; a long value is cut short.
  value: { maxWidth: '50%', textAlign: 'right' },
  chevron: { transform: [{ rotate: '180deg' }] },
  slider: { minHeight: ROW_HEIGHT, paddingVertical: space.sm, gap: space.xs },
  line: { flexDirection: 'row', alignItems: 'center', gap: space.md },
  stacked: { minHeight: ROW_HEIGHT, paddingVertical: space.sm, gap: space.xs },
  singleLine: { flex: 1, textAlign: 'right', paddingVertical: 0 },
  multiline: { minHeight: 88, paddingVertical: 0, textAlignVertical: 'top' },
});
