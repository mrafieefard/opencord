// "What should people call you?": the last step of both ways in.
import { useState } from 'react';
import { StyleSheet, TextInput, View } from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { describeError } from '@/core';
import { Button, Text, radius, space, typography, useTheme } from '@/ui';
import { MAX_FONT_SCALE } from '@/ui/theme/typography';

import { createIdentity } from './identity';
import { peekSecret, takeSecret } from './pending';

export function CreateScreen() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const imported = peekSecret() !== null;

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      await createIdentity(name, takeSecret() ?? undefined);
    } catch (failure: unknown) {
      setError(describeError(failure));
      setBusy(false);
    }
  }

  return (
    <KeyboardAvoidingView behavior="padding" style={[styles.screen, { backgroundColor: colors.chat }]}>
      <View style={[styles.body, { paddingTop: insets.top + space.xxxl }]}>
        <Text variant="title1">What should people call you?</Text>
        <Text variant="callout" color="textSecondary">
          {imported ? 'Your identity is ready. Pick the name servers show.' : 'You can change it later in You.'}
        </Text>
        <TextInput
          value={name}
          onChangeText={setName}
          placeholder="Display name"
          placeholderTextColor={colors.textMuted}
          autoFocus
          maxLength={64}
          returnKeyType="done"
          onSubmitEditing={() => void submit()}
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
        <Button title="Continue" variant="primary" loading={busy} onPress={() => void submit()} />
      </View>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  body: { flex: 1, paddingHorizontal: space.lg, gap: space.md },
  input: { borderRadius: radius.icon, paddingHorizontal: space.lg, paddingVertical: space.md, marginTop: space.lg },
  footer: { paddingHorizontal: space.lg },
});
