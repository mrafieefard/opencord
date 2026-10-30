// The voice connection in iOS's tab bar accessory (iOS 26), where Apple
// Music shows what plays: above the tab bar, in the system's glass. Inline
// (the tab bar minimised) it keeps only the channel and the mic.
import { NativeTabs } from 'expo-router/native-tabs';
import { StyleSheet } from 'react-native';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';

import { useMyVoice } from '@/state/selectors';
import { space } from '@/ui';

import { callProgress } from './callPosition';
import { ConnectedVoiceRow } from './ConnectedBar';

export function VoiceAccessory() {
  const myVoice = useMyVoice();
  const placement = NativeTabs.BottomAccessory.usePlacement();
  // The call's glass grows out of this bar and covers it: from the first
  // moment, the bar shows nothing of its own, so only the growing glass is seen.
  const away = useAnimatedStyle(() => ({ opacity: callProgress.get() > 0.001 ? 0 : 1 }));
  if (!myVoice) return null;
  return (
    <Animated.View style={[styles.accessory, away]}>
      <ConnectedVoiceRow
        serverKey={myVoice.serverKey}
        channelId={myVoice.channelId}
        muted={myVoice.muted}
        compact={placement === 'inline'}
      />
    </Animated.View>
  );
}

const styles = StyleSheet.create({
  accessory: { flex: 1, flexDirection: 'row', alignItems: 'center', paddingLeft: space.lg, paddingRight: space.xs },
});
