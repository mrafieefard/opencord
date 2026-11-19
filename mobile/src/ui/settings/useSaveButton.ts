// `Save` on the right of a settings page's native header: pressable only when
// something changed and no save is running, a spinner in its place while one
// runs. A failed save is a toast with the server's words (`report`), and Save
// can be pressed again.
import { useNavigation } from 'expo-router';
import { createElement, useLayoutEffect, useRef } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { report } from '@/core/report';

import { Text } from '../Text';
import { space, TOUCH } from '../theme/space';
import { useTheme } from '../theme/useTheme';

export interface SaveButtonOptions {
  dirty: boolean;
  saving: boolean;
  onSave: () => Promise<void>;
}

export function useSaveButton({ dirty, saving, onSave }: SaveButtonOptions): void {
  const navigation = useNavigation();
  // A second press before the page's `saving` turns true would save twice.
  const running = useRef(false);
  useLayoutEffect(() => {
    const enabled = dirty && !saving;
    const save = () => {
      // The button drops its onPress when disabled; this guards any other caller.
      if (!enabled || running.current) return;
      running.current = true;
      report(
        onSave().finally(() => {
          running.current = false;
        }),
        'save settings',
      );
    };
    navigation.setOptions({
      headerRight: () => createElement(SaveButton, { enabled, saving, onPress: save }),
    });
  }, [navigation, dirty, saving, onSave]);
}

interface SaveButtonProps {
  enabled: boolean;
  saving: boolean;
  onPress: () => void;
}

/**
 * Words in the header, as the system's own bar buttons are. `Button` is a
 * full-width action and `IconButton` a glyph, so neither fits here; it is
 * built from the same Touchable as they are, which the Servers header
 * already uses in a native header.
 */
function SaveButton({ enabled, saving, onPress }: SaveButtonProps) {
  const { colors } = useTheme();
  return createElement(
    Touchable,
    {
      onPress: enabled ? onPress : undefined,
      disabled: !enabled,
      hitSlop: space.sm,
      underlayColor: colors.selected,
      activeUnderlayOpacity: 1,
      style: styles.button,
    },
    createElement(
      View,
      {
        accessible: true,
        accessibilityRole: 'button',
        accessibilityLabel: 'Save',
        accessibilityState: { disabled: !enabled, busy: saving },
        style: styles.content,
      },
      saving
        ? createElement(ActivityIndicator, { testID: 'save-spinner', color: colors.textMuted })
        : createElement(Text, { variant: 'headline', color: enabled ? 'text' : 'textMuted' }, 'Save'),
    ),
  );
}

const styles = StyleSheet.create({
  button: { minHeight: TOUCH, minWidth: TOUCH, borderRadius: TOUCH / 2, justifyContent: 'center' },
  content: { alignItems: 'center', justifyContent: 'center', paddingHorizontal: space.sm },
});
