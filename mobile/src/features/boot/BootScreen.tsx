// While the core opens: the wordmark on the chat color. A failure shows
// its sentence with Try again (design §8.1).
import { StyleSheet, View } from 'react-native';

import { Button, Text, space, useTheme } from '@/ui';

export function BootScreen({ failure, onRetry }: { failure?: string; onRetry?: () => void }) {
  const { colors } = useTheme();
  return (
    <View style={[styles.screen, { backgroundColor: colors.chat }]}>
      <Text variant="display">Opencord</Text>
      {failure !== undefined && (
        <View style={styles.failure}>
          <Text variant="callout" color="textSecondary" style={styles.center}>
            {failure}
          </Text>
          {onRetry && <Button title="Try again" variant="primary" onPress={onRetry} />}
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: space.xxxl },
  failure: { alignSelf: 'stretch', gap: space.lg, marginTop: space.xxl },
  center: { textAlign: 'center' },
});
