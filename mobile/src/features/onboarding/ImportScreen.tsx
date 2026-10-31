// A backup file from the desktop: picked, read, decoded; then the name.
// Welcome opens the picker itself; this screen is for a wrong file.
import * as DocumentPicker from 'expo-document-picker';
import { File } from 'expo-file-system';
import { router, useLocalSearchParams } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Button, Text, space, useTheme } from '@/ui';

import { readBackup } from './identity';
import { holdSecret } from './pending';

export const NOT_A_BACKUP = "That isn't an Opencord backup.";

export async function pickBackup(): Promise<'picked' | 'cancelled'> {
  const result = await DocumentPicker.getDocumentAsync({ type: '*/*', copyToCacheDirectory: true, multiple: false });
  const asset = result.canceled ? undefined : result.assets[0];
  if (!asset) return 'cancelled';
  const file = new File(asset.uri);
  try {
    holdSecret(readBackup(await file.text()));
  } finally {
    file.delete();
  }
  return 'picked';
}

export function ImportScreen() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { failed } = useLocalSearchParams<{ failed?: string }>();
  const [error, setError] = useState<string | null>(failed === '1' ? NOT_A_BACKUP : null);

  async function choose() {
    try {
      const picked = await pickBackup();
      setError(null);
      if (picked === 'picked') router.replace('/welcome/create');
    } catch {
      setError(NOT_A_BACKUP);
    }
  }

  return (
    <View style={[styles.screen, { backgroundColor: colors.chat, paddingTop: insets.top + space.xxxl, paddingBottom: insets.bottom + space.lg }]}>
      <View style={styles.body}>
        <Text variant="title1">Import a backup</Text>
        <Text variant="callout" color="textSecondary">
          Choose the opencord-identity file you saved from Opencord on another device.
        </Text>
        {error !== null && (
          <Text variant="footnote" color="recording" accessibilityRole="alert">
            {error}
          </Text>
        )}
      </View>
      <Button title="Choose the file" variant="primary" onPress={() => void choose()} />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, paddingHorizontal: space.lg },
  body: { flex: 1, gap: space.md },
});
