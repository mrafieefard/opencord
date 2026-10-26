import { router } from 'expo-router';
import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Button, Text, space, useTheme } from '@/ui';

import { pickBackup } from './ImportScreen';
import { holdSecret } from './pending';

async function importBackup(): Promise<void> {
  try {
    if ((await pickBackup()) === 'picked') router.push('/welcome/create');
  } catch {
    router.push({ pathname: '/welcome/import', params: { failed: '1' } });
  }
}

export function WelcomeScreen() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <View style={[styles.screen, { backgroundColor: colors.chat, paddingTop: insets.top, paddingBottom: insets.bottom + space.lg }]}>
      <View style={styles.hero}>
        <Text variant="display">Opencord</Text>
        <Text variant="callout" color="textSecondary">
          Chat and voice on servers you trust.
        </Text>
      </View>
      <View style={styles.actions}>
        <Button
          title="Create an identity"
          variant="primary"
          onPress={() => {
            holdSecret(null);
            router.push('/welcome/create');
          }}
        />
        <Button title="Import a backup" variant="secondary" onPress={() => void importBackup()} />
        <Text variant="footnote" color="textMuted" style={styles.center}>
          Your identity is a key that stays on this phone.
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, paddingHorizontal: space.lg },
  hero: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: space.sm },
  actions: { gap: space.md },
  center: { textAlign: 'center', marginTop: space.sm },
});
