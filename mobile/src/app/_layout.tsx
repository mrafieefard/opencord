// The root: providers, the boot gate, and the native stack. Routes need
// the boot state: onboarding before an identity, the rest after.
import { DarkTheme, DefaultTheme, Stack, ThemeProvider as NavigationTheme } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { LogBox, useWindowDimensions } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { BootGate } from '@/features/boot/BootGate';
import { DETENTS, TWO_COLUMNS } from '@/features/music/sheet';
import { SHEET_DETENTS } from '@/features/serverMenu/ServerSheet';
import { CallSheet } from '@/features/voice/CallSheet';
import { useVoiceCues } from '@/features/voice/useVoiceCues';
import { useBoot } from '@/state/selectors';
import { DialogHost, ThemeProvider, ToastHost, sheetLook, useTheme } from '@/ui';

// Suppress library warnings from expo-router:
// - useClearGuardedRoutes, set off by Stack.Protected guards during initial redirect.
// - formSheet warning when deep-linking directly into a modal sheet as the initial screen.
// - tab preloading action warning during deep link navigation.
LogBox.ignoreLogs([/REMOVE_ROUTES/, /uses presentation.*first screen/, /The action 'PRELOAD'/]);

function Routes() {
  useVoiceCues();
  const boot = useBoot();
  const { colors, scheme } = useTheme();
  const { width } = useWindowDimensions();
  const ready = boot.state === 'ready';
  // What shows behind a screen while it moves (the corners of one swiped
  // back on iOS 26): React Navigation's light grey unless it is given ours.
  const base = scheme === 'dark' ? DarkTheme : DefaultTheme;
  const navigationTheme = {
    ...base,
    colors: { ...base.colors, background: colors.chat, card: colors.chat, text: colors.text, border: colors.border, primary: colors.text },
  };
  return (
    <NavigationTheme value={navigationTheme}>
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
              sheetCornerRadius: sheetLook.cornerRadius,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/members"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: SHEET_DETENTS,
              sheetInitialDetentIndex: 0,
              sheetGrabberVisible: true,
              sheetCornerRadius: sheetLook.cornerRadius,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/member/[userId]"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: 'fitToContents',
              sheetGrabberVisible: true,
              sheetCornerRadius: sheetLook.cornerRadius,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/settings"
            options={{
              presentation: 'formSheet',
              // Medium; the sheet's stack allows only large while a page is open.
              sheetAllowedDetents: SHEET_DETENTS,
              sheetInitialDetentIndex: 0,
              sheetGrabberVisible: true,
              sheetCornerRadius: sheetLook.cornerRadius,
              // A grouped settings list, opaque as iOS Settings: its cards are `surface` on `chat`.
              contentStyle: { backgroundColor: colors.chat },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/voice/[channelId]"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: 'fitToContents',
              sheetGrabberVisible: true,
              sheetCornerRadius: sheetLook.cornerRadius,
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
              sheetCornerRadius: sheetLook.cornerRadius,
              // The player stays solid, as Apple Music's: its rows are.
              contentStyle: { backgroundColor: colors.elevated },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/soundboard/[channelId]"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: DETENTS,
              sheetInitialDetentIndex: 0,
              sheetGrabberVisible: true,
              sheetCornerRadius: sheetLook.cornerRadius,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/activities/[channelId]"
            options={{
              presentation: 'formSheet',
              sheetAllowedDetents: DETENTS,
              sheetInitialDetentIndex: 0,
              sheetGrabberVisible: true,
              sheetCornerRadius: sheetLook.cornerRadius,
              contentStyle: { backgroundColor: sheetLook.background(colors) },
            }}
          />
          <Stack.Screen
            name="server/[serverKey]/activity/[sessionId]"
            options={{
              // Over the call (iOS draws modals above it): the activity's own page.
              presentation: 'fullScreenModal',
              contentStyle: { backgroundColor: colors.chat },
            }}
          />
          <Stack.Screen
            name="viewer"
            options={{ presentation: 'transparentModal', headerShown: false, animation: 'fade' }}
          />
          <Stack.Screen
            name="text-preview"
            options={{ presentation: 'modal', headerShown: false }}
          />
        </Stack.Protected>
        <Stack.Screen name="dev/gallery" />
        <Stack.Screen name="dev/plugin-area" />
      </Stack>
      {ready && <CallSheet />}
      <ToastHost />
      <DialogHost />
    </NavigationTheme>
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
