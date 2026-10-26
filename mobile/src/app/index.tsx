// A first screen that proves the Rust core loads (plan Task 3); Task 7
// replaces it with the boot gate and onboarding.
import { coreVersion } from '@opencord/core-native';
import { StyleSheet, Text, View } from 'react-native';

export default function First() {
  return (
    <View style={styles.screen}>
      <Text style={styles.title}>Opencord</Text>
      <Text style={styles.version}>core {coreVersion()}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#0E0E10' },
  title: { color: '#EDEDEF', fontSize: 34, fontWeight: '700' },
  version: { color: '#A1A1A8', fontSize: 15, marginTop: 8 },
});
