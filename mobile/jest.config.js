/** @type {import('jest').Config} */
module.exports = {
  preset: 'jest-expo',
  setupFiles: ['<rootDir>/src/test/setup.ts'],
  testPathIgnorePatterns: ['/node_modules/', '/android/', '/ios/', '/packages/'],
  moduleNameMapper: {
    '^@/(.*)$': '<rootDir>/src/$1',
    // Reanimated and Worklets need a native runtime: a small mock of what
    // the app uses stands in (src/test/reanimated.tsx).
    '^react-native-reanimated$': '<rootDir>/src/test/reanimated.tsx',
    '^react-native-worklets$': '<rootDir>/src/test/worklets.ts',
    // The generated bindings without the native installer: their enums and
    // constructors work, and calls reach native code only when made (tests
    // use FakeCore instead).
    '^@opencord/core-native$': '<rootDir>/packages/core-native/src/generated/opencord_mobile.ts',
  },
  transformIgnorePatterns: [
    'node_modules/(?!((jest-)?react-native|@react-native(-community)?|expo(nent)?|@expo(nent)?/.*|@expo-google-fonts/.*|react-navigation|@react-navigation/.*|@sentry/react-native|native-base|react-native-svg|@ubjs/.*|uniffi-bindgen-react-native|@shopify/.*))',
  ],
};
