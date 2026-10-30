// The voice connection in iOS's tab bar accessory (iOS 26), where Apple
// Music shows what plays: above the tab bar, in the system's glass. Inline
// (the tab bar minimised) it keeps only the channel and the mic.
import { NativeTabs } from 'expo-router/native-tabs';
import { StyleSheet, View } from 'react-native';

import { useMyVoice } from '@/state/selectors';
import { space } from '@/ui';

import { ConnectedVoiceRow } from './ConnectedBar';

export function VoiceAccessory() {
  const myVoice = useMyVoice();
  const placement = NativeTabs.BottomAccessory.usePlacement();
  if (!myVoice) return null;
  return (
    <View style={styles.accessory}>
      <ConnectedVoiceRow
        serverKey={myVoice.serverKey}
        channelId={myVoice.channelId}
        muted={myVoice.muted}
        compact={placement === 'inline'}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  accessory: { flex: 1, flexDirection: 'row', alignItems: 'center', paddingLeft: space.lg, paddingRight: space.xs },
});
