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

// expo-sharing in memory.
jest.mock('expo-sharing', () => ({
  isAvailableAsync: jest.fn(async () => true),
  shareAsync: jest.fn(async () => {}),
}));

// expo-file-system in memory.
const mockFileSystemFiles = new Map<string, string>();
jest.mock('expo-file-system', () => ({
  Paths: {
    document: { uri: 'file:///data/' },
    cache: { uri: 'file:///cache/' },
  },
  File: class MockFile {
    readonly uri: string;
    constructor(...parts: unknown[]) {
      this.uri = parts
        .map((p) => (typeof p === 'object' && p && 'uri' in p ? (p as { uri: string }).uri : String(p)))
        .join('/')
        .replace(/\/+/g, '/');
    }
    async text(): Promise<string> {
      return mockFileSystemFiles.get(this.uri) ?? '';
    }
    async write(content: string): Promise<void> {
      mockFileSystemFiles.set(this.uri, content);
    }
    delete(): void {
      mockFileSystemFiles.delete(this.uri);
    }
  },
  __mockFiles: mockFileSystemFiles,
}));

// expo-constants mock.
jest.mock('expo-constants', () => ({
  expoConfig: { version: '0.0.1' },
}));

// @expo/ui swift-ui mocks for Jest.
jest.mock('@expo/ui/swift-ui', () => {
  const React = require('react');
  const { View, Text, TouchableOpacity } = require('react-native');
  return {
    Host: ({ children, style }: { children: React.ReactNode; style?: unknown }) => React.createElement(View, { style }, children),
    Text: ({ children, ...props }: { children?: React.ReactNode; [key: string]: unknown }) => React.createElement(Text, props, children),
    Toggle: ({ isOn, onIsOnChange, label }: { isOn: boolean; onIsOnChange?: (v: boolean) => void; label: string }) =>
      React.createElement(TouchableOpacity, {
        accessibilityRole: 'switch',
        accessibilityLabel: label,
        accessibilityState: { checked: isOn },
        onPress: () => onIsOnChange?.(!isOn),
        onChange: (val: boolean) => onIsOnChange?.(val),
      }),
    Slider: ({ value, onValueChange, modifiers }: { value: number; onValueChange?: (v: number) => void; modifiers?: { _label?: string }[] }) => {
      const labelMod = modifiers?.find((m) => m && m._label);
      const label = labelMod?._label;
      return React.createElement(View, {
        accessibilityRole: 'adjustable',
        accessibilityLabel: label,
        value,
        onChange: (v: number) => onValueChange?.(v),
      });
    },
    Picker: ({ selection, onSelectionChange, children }: { selection: number; onSelectionChange?: (i: number) => void; children: React.ReactNode }) => {
      return React.createElement(
        View,
        { accessibilityRole: 'radiogroup' },
        React.Children.map(children, (child: unknown, idx: number) => {
          const c = child as { props?: { children?: string; modifiers?: { _tag?: number }[] } } | null;
          const val = c?.props?.children;
          const tagMod = c?.props?.modifiers?.find((m) => m && m._tag !== undefined);
          const pos = tagMod?._tag ?? idx;
          return React.createElement(
            TouchableOpacity,
            {
              key: val ?? pos,
              accessibilityRole: 'radio',
              accessibilityLabel: `Theme ${val}`,
              accessibilityState: { selected: selection === pos },
              onPress: () => onSelectionChange?.(pos),
            },
            c,
          );
        }),
      );
    },
  };
});

jest.mock('@expo/ui/swift-ui/modifiers', () => ({
  pickerStyle: () => ({ _style: 'segmented' }),
  tag: (idx: number) => ({ _tag: idx }),
  accessibilityLabel: (lbl: string) => ({ _label: lbl }),
  disabled: (d: boolean) => ({ _disabled: d }),
  tint: (t: string) => ({ _tint: t }),
  labelsHidden: () => ({ _labelsHidden: true }),
}));

// Gesture Handler's own Jest setup, FlashList setup, and safe areas without a native view.
require('react-native-gesture-handler/jestSetup');
require('@shopify/flash-list/jestSetup');
jest.mock('react-native-safe-area-context', () => require('react-native-safe-area-context/jest/mock').default);
jest.mock('react-native-keyboard-controller', () => require('react-native-keyboard-controller/jest'));
