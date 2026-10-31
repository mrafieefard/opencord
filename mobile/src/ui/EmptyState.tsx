import { StyleSheet, View } from 'react-native';

import { Button } from './Button';
import { Text } from './Text';
import { space } from './theme/space';

export interface EmptyStateProps {
  title: string;
  body?: string;
  action?: { title: string; onPress: () => void };
}

export function EmptyState({ title, body, action }: EmptyStateProps) {
  return (
    <View style={styles.box}>
      <Text variant="headline" style={styles.center}>
        {title}
      </Text>
      {body !== undefined && (
        <Text variant="callout" color="textSecondary" style={styles.center}>
          {body}
        </Text>
      )}
      {action && (
        <View style={styles.action}>
          <Button title={action.title} onPress={action.onPress} variant="primary" />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  box: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: space.xxxl, gap: space.sm },
  center: { textAlign: 'center' },
  action: { alignSelf: 'stretch', marginTop: space.lg },
});
