import { StyleSheet, View } from 'react-native';

import { PresenceStatus } from '@/core/native';
import { type ColorToken } from '@/ui/theme/tokens';
import { useTheme } from '@/ui/theme/useTheme';

export interface PresenceDotProps {
  presence?: PresenceStatus;
  size?: number;
  ringColor?: ColorToken;
}

const presenceLabels: Record<PresenceStatus, string> = {
  [PresenceStatus.Online]: 'Online',
  [PresenceStatus.Idle]: 'Idle',
  [PresenceStatus.Dnd]: 'Do not disturb',
  [PresenceStatus.Offline]: 'Offline',
};

export function PresenceDot({ presence = PresenceStatus.Offline, size = 10, ringColor = 'sidebar' }: PresenceDotProps) {
  const { colors } = useTheme();
  const ring = 2;
  const outerSize = size + ring * 2;
  const outerRadius = outerSize / 2;
  const innerRadius = size / 2;
  const ringBg = colors[ringColor] ?? colors.sidebar;
  const label = presenceLabels[presence] ?? 'Offline';

  return (
    <View
      accessible
      accessibilityLabel={label}
      style={[
        styles.outer,
        {
          width: outerSize,
          height: outerSize,
          borderRadius: outerRadius,
          backgroundColor: ringBg,
        },
      ]}
    >
      {presence === PresenceStatus.Online && (
        <View
          style={[
            styles.dot,
            {
              width: size,
              height: size,
              borderRadius: innerRadius,
              backgroundColor: colors.text,
            },
          ]}
        />
      )}
      {presence === PresenceStatus.Idle && (
        <View
          style={[
            styles.dot,
            styles.clipped,
            {
              width: size,
              height: size,
              borderRadius: innerRadius,
              backgroundColor: colors.text,
            },
          ]}
        >
          <View
            style={[
              styles.crescentBite,
              {
                width: size * 0.7,
                height: size * 0.7,
                borderRadius: (size * 0.7) / 2,
                backgroundColor: ringBg,
                top: -size * 0.2,
                left: -size * 0.2,
              },
            ]}
          />
        </View>
      )}
      {presence === PresenceStatus.Dnd && (
        <View
          style={[
            styles.dot,
            styles.center,
            {
              width: size,
              height: size,
              borderRadius: innerRadius,
              backgroundColor: colors.text,
            },
          ]}
        >
          <View
            style={{
              width: size * 0.7,
              height: Math.max(2, size * 0.22),
              borderRadius: 1,
              backgroundColor: ringBg,
            }}
          />
        </View>
      )}
      {presence === PresenceStatus.Offline && (
        <View
          style={{
            width: size,
            height: size,
            borderRadius: innerRadius,
            borderWidth: Math.max(1.5, Math.round(size * 0.2)),
            borderColor: colors.textMuted,
            backgroundColor: 'transparent',
          }}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  outer: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  dot: {
    overflow: 'hidden',
  },
  clipped: {
    position: 'relative',
  },
  crescentBite: {
    position: 'absolute',
  },
  center: {
    alignItems: 'center',
    justifyContent: 'center',
  },
});
