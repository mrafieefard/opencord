/// <reference types="jest" />
// Jest setup. @opencord/core-native maps to the generated bindings without
// their native installer (jest.config.js); tests use FakeCore for calls.

// expo-secure-store, kept in memory.
jest.mock('expo-secure-store', () => {
  const items = new Map<string, string>();
  return {
    AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY: 0,
    getItemAsync: jest.fn(async (key: string) => items.get(key) ?? null),
    setItemAsync: jest.fn(async (key: string, value: string) => {
      items.set(key, value);
    }),
    deleteItemAsync: jest.fn(async (key: string) => {
      items.delete(key);
    }),
    __items: items,
  };
});


// expo-clipboard in memory.
let mockClipboardContent = '';
jest.mock('expo-clipboard', () => ({
  getStringAsync: jest.fn(async () => mockClipboardContent),
  setStringAsync: jest.fn(async (text: string) => {
    mockClipboardContent = text;
    return true;
  }),
  __setContent: (text: string) => {
    mockClipboardContent = text;
  },
}));

// Gesture Handler's own Jest setup, FlashList setup, and safe areas without a native view.
require('react-native-gesture-handler/jestSetup');
require('@shopify/flash-list/jestSetup');
jest.mock('react-native-safe-area-context', () => require('react-native-safe-area-context/jest/mock').default);
jest.mock('react-native-keyboard-controller', () => require('react-native-keyboard-controller/jest'));
