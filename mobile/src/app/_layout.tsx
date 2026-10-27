// The root: providers, the boot gate, and the native stack. Routes need
// the boot state: onboarding before an identity, the rest after.
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { BootGate } from '@/features/boot/BootGate';
import { useBoot } from '@/state/selectors';
import { ThemeProvider, ToastHost, useTheme } from '@/ui';

function Routes() {
  const boot = useBoot();
  const { colors, scheme } = useTheme();
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
            options={{ presentation: 'formSheet', sheetAllowedDetents: [0.6], sheetGrabberVisible: true }}
          />
          <Stack.Screen name="server/[serverKey]/index" />
          <Stack.Screen
            name="viewer"
            options={{ presentation: 'transparentModal', headerShown: false, animation: 'fade' }}
          />
        </Stack.Protected>
        <Stack.Screen name="dev/gallery" />
      </Stack>
      <ToastHost />
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
