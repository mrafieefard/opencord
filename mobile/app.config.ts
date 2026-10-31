// The phone apps' configuration (design docs/superpowers/specs/2026-10-26-mobile-client-design.md §11).
// `expo prebuild` makes mobile/android and mobile/ios from this; they aren't committed.
import type { ExpoConfig } from 'expo/config';

// With their extensions: Expo loads this file the way Node does.
import { versionCode } from './plugins/versionCode.ts';
import { withReleaseSigning } from './plugins/withReleaseSigning.ts';

const fonts = [
  './assets/fonts/Inter-Regular.ttf',
  './assets/fonts/Inter-Medium.ttf',
  './assets/fonts/Inter-SemiBold.ttf',
  './assets/fonts/Inter-Bold.ttf',
  './assets/fonts/Inter-Italic.ttf',
  './assets/fonts/Inter-SemiBoldItalic.ttf',
  './assets/fonts/JetBrainsMono-Regular.ttf',
];

// CI sets the release version (docs/releasing.md); a local build is 0.0.1.
const version = process.env.OPENCORD_VERSION || '0.0.1';

const microphone = 'Opencord uses the microphone in voice channels.';
const photos = 'Opencord lets you send photos from your library.';

const config: ExpoConfig = {
  name: 'Opencord',
  slug: 'opencord',
  version,
  orientation: 'default',
  icon: './assets/icon.png',
  // The app's own links first, then the invite links +native-intent.tsx
  // turns into the Add server sheet.
  scheme: ['opencord-app', 'opencord', 'opencords'],
  userInterfaceStyle: 'automatic',
  backgroundColor: '#0E0E10',
  ios: {
    bundleIdentifier: 'dev.opencord.mobile',
    supportsTablet: true,
    infoPlist: {
      NSMicrophoneUsageDescription: microphone,
      NSPhotoLibraryUsageDescription: photos,
      UIBackgroundModes: ['audio'],
    },
  },
  android: {
    package: 'dev.opencord.mobile',
    versionCode: versionCode(version),
    adaptiveIcon: {
      foregroundImage: './assets/adaptive-icon.png',
      backgroundColor: '#0E0E10',
    },
    permissions: [
      'android.permission.INTERNET',
      'android.permission.RECORD_AUDIO',
      'android.permission.FOREGROUND_SERVICE',
      'android.permission.FOREGROUND_SERVICE_MICROPHONE',
      'android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK',
      'android.permission.POST_NOTIFICATIONS',
      'android.permission.WAKE_LOCK',
    ],
    predictiveBackGestureEnabled: false,
  },
  plugins: [
    'expo-router',
    ['expo-font', { fonts }],
    'expo-secure-store',
    ['expo-audio', { microphonePermission: microphone }],
    // `false` here would block RECORD_AUDIO for the whole app and drop iOS's
    // microphone sentence, so voice gets the same sentence instead.
    ['expo-image-picker', { photosPermission: photos, cameraPermission: false, microphonePermission: microphone }],
    [
      'expo-splash-screen',
      { backgroundColor: '#F3F3F5', dark: { backgroundColor: '#0E0E10' } },
    ],
    [
      'expo-build-properties',
      {
        // cpal's AAudio back end needs Android 8.0.
        android: { minSdkVersion: 26 },
        // @react-native-menu/menu 2.0.0 imports "RCTBridge.h" as a local
        // header, which the precompiled React core doesn't provide.
        ios: { buildReactNativeFromSource: true },
      },
    ],
  ],
  experiments: {
    typedRoutes: true,
    reactCompiler: true,
  },
};

export default withReleaseSigning(config);
