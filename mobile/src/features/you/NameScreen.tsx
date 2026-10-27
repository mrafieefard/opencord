// Changing the user's display name: saved locally and broadcast to connected servers.
import { useState } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { describeError } from '@/core';
import { setDisplayName } from '@/state/actions/profile';
import { useSelf } from '@/state/selectors';
import { Button, Text, radius, space, typography, useTheme } from '@/ui';
import { MAX_FONT_SCALE } from '@/ui/theme/typography';

export function NameScreen() {
  const router = useRouter();
  const self = useSelf();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const [name, setName] = useState(self?.displayName ?? '');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    const trimmed = name.trim();
    if (!trimmed) {
      setError('display name must not be empty');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await setDisplayName(trimmed);
      router.back();
    } catch (failure: unknown) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <KeyboardAvoidingView behavior="padding" style={[styles.screen, { backgroundColor: colors.chat }]}>
      <View style={[styles.body, { paddingTop: insets.top + space.lg }]}>
        <Text variant="callout" color="textSecondary">
          This is what people on servers see. You can change it at any time.
        </Text>
        <TextInput
          value={name}
          onChangeText={setName}
          placeholder="Display name"
          placeholderTextColor={colors.textMuted}
          autoFocus
          maxLength={64}
          returnKeyType="done"
          onSubmitEditing={() => void save()}
          maxFontSizeMultiplier={MAX_FONT_SCALE}
          accessibilityLabel="Display name"
          style={[typography.body, styles.input, { color: colors.text, backgroundColor: colors.surface }]}
        />
        {error !== null && (
          <Text variant="footnote" color="recording" accessibilityRole="alert">
            {error}
          </Text>
        )}
      </View>
      <View style={[styles.footer, { paddingBottom: insets.bottom + space.lg }]}>
        <Button
          title="Save"
          variant="primary"
          loading={busy}
          disabled={!name.trim() || name.trim() === self?.displayName}
          onPress={() => void save()}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  body: { flex: 1, paddingHorizontal: space.lg, gap: space.md },
  input: {
    borderRadius: radius.icon,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
  },
  footer: { paddingHorizontal: space.lg },
});
