import { useRef, type ComponentRef, type ReactNode } from 'react';
import { Pressable, StyleSheet, TextInput } from 'react-native';

import { Icon } from './Icon';
import { IconButton } from './IconButton';
import { MAX_FONT_SCALE, typography } from './theme/typography';
import { radius, space } from './theme/space';
import { useTheme } from './theme/useTheme';

export interface SearchFieldProps {
  value: string;
  onChangeText: (text: string) => void;
  placeholder: string;
  onSubmit?: () => void;
  right?: ReactNode;
  autoFocus?: boolean;
  /** On a sheet (`elevated`), where `hover` would sink below it. */
  onSheet?: boolean;
}

export function SearchField({ value, onChangeText, placeholder, onSubmit, right, autoFocus = false, onSheet = false }: SearchFieldProps) {
  const { colors } = useTheme();
  const inputRef = useRef<ComponentRef<typeof TextInput>>(null);
  return (
    <Pressable
      onPress={() => inputRef.current?.focus()}
      style={[styles.field, { backgroundColor: onSheet ? colors.selected : colors.hover }]}
    >
      <Icon name="search" size={16} color="textMuted" />
      <TextInput
        ref={inputRef}
        value={value}
        onChangeText={onChangeText}
        placeholder={placeholder}
        placeholderTextColor={colors.textMuted}
        onSubmitEditing={onSubmit}
        returnKeyType="search"
        autoFocus={autoFocus}
        autoCorrect={false}
        maxFontSizeMultiplier={MAX_FONT_SCALE}
        accessibilityLabel={placeholder}
        style={[typography.body, styles.input, { color: colors.text }]}
      />
      {value !== '' && <IconButton icon="close" label="Clear" onPress={() => onChangeText('')} size={36} />}
      {right}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  field: { flexDirection: 'row', alignItems: 'center', gap: space.sm, paddingLeft: space.md, borderRadius: radius.icon, minHeight: 44 },
  input: { flex: 1, paddingVertical: space.sm },
});
