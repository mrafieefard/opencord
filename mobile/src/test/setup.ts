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
