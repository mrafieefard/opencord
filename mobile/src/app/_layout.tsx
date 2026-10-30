// The root: providers, the boot gate, and the native stack. Routes need
// the boot state: onboarding before an identity, the rest after.
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useWindowDimensions } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { BootGate } from '@/features/boot/BootGate';
import { DETENTS, TWO_COLUMNS } from '@/features/music/sheet';
import { useBoot } from '@/state/selectors';
import { DialogHost, ThemeProvider, ToastHost, radius, sheetLook, useTheme } from '@/ui';

function Routes() {
  const boot = useBoot();
  const { colors, scheme } = useTheme();
  const { width } = useWindowDimensions();
  const ready = boot.state === 'ready';
  return (
    <>
      <StatusBar style={scheme === 'dark' ? 'light' : 'dark'} />
      <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: colors.chat } }}>
        <Stack.Screen name="index" />
        <Stack.Protected guard={!ready}>
          <Stack.Screen name="welcome" />
        </Stack.Protected>
        <Stack.Protected guard={ready}>
          <Stack.Screen name="(tabs)" />
          <Stack.Screen
            name="add-server"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: 'fitToContents',
              sheetGrabberVisible: true,
              sheetCornerRadius: radius.sheet,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen name="server/[serverKey]/index" />
          <Stack.Screen
            name="server/[serverKey]/member/[userId]"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: 'fitToContents',
              sheetGrabberVisible: true,
              sheetCornerRadius: radius.sheet,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/music/[channelId]"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: DETENTS,
              // Half on a phone; full when the player takes two columns.
              sheetInitialDetentIndex: width >= TWO_COLUMNS ? 'last' : 0,
              sheetGrabberVisible: true,
              sheetCornerRadius: radius.sheet,
              // The player stays solid, as Apple Music's: its rows are.
              contentStyle: { backgroundColor: colors.elevated },
            }}
          />
          <Stack.Screen
            name="viewer"
            options={{ presentation: 'transparentModal', headerShown: false, animation: 'fade' }}
          />
        </Stack.Protected>
        <Stack.Screen name="dev/gallery" />
      </Stack>
      <ToastHost />
      <DialogHost />
    </>
  );
}

export default function RootLayout() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <KeyboardProvider>
          <ThemeProvider>
            <BootGate>
              <Routes />
            </BootGate>
          </ThemeProvider>
        </KeyboardProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
