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

// expo-image-picker in memory.
jest.mock('expo-image-picker', () => ({
  MediaTypeOptions: { Images: 'Images', All: 'All', Videos: 'Videos' },
  launchImageLibraryAsync: jest.fn(async () => ({ canceled: true, assets: [] })),
}));

// expo-document-picker in memory.
jest.mock('expo-document-picker', () => ({
  getDocumentAsync: jest.fn(async () => ({ canceled: true, assets: [] })),
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

// expo-audio mock.
jest.mock('expo-audio', () => ({
  getRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestNotificationPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  setAudioModeAsync: jest.fn(async () => {}),
}));

// @expo/ui swift-ui mocks for Jest.
jest.mock('@expo/ui/swift-ui', () => {
  const React = require('react');
  const { View, Text, Pressable } = require('react-native');
  // A context menu shows its items beside its trigger, as menu items.
  const ContextMenu = ({ children }: { children: React.ReactNode }) => React.createElement(View, null, children);
  ContextMenu.Items = function Items({ children }: { children: React.ReactNode }) {
    return React.createElement(View, null, children);
  };
  ContextMenu.Trigger = function Trigger({ children }: { children: React.ReactNode }) {
    return React.createElement(React.Fragment, null, children);
  };
  return {
    Host: ({ children, style }: { children: React.ReactNode; style?: unknown }) => React.createElement(View, { style }, children),
    ContextMenu,
    RNHostView: ({ children }: { children: React.ReactNode }) => children,
    Button: ({ label, role, onPress, modifiers }: { label?: string; role?: string; onPress?: () => void; modifiers?: { _disabled?: boolean }[] }) =>
      React.createElement(Pressable, {
        accessibilityRole: 'menuitem',
        accessibilityLabel: label,
        accessibilityHint: role === 'destructive' ? 'destructive' : undefined,
        disabled: modifiers?.some((m) => m && m._disabled) ?? false,
        onPress,
      }),
    Text: ({ children, ...props }: { children?: React.ReactNode; [key: string]: unknown }) => React.createElement(Text, props, children),
    Toggle: ({ isOn, onIsOnChange, label }: { isOn: boolean; onIsOnChange?: (v: boolean) => void; label: string }) =>
      React.createElement(Pressable, {
        accessibilityRole: 'switch',
        accessibilityLabel: label,
        accessibilityState: { checked: isOn },
        onPress: () => onIsOnChange?.(!isOn),
        onChange: (val: boolean) => onIsOnChange?.(val),
      }),
    Slider: ({ value, onValueChange, onEditingChanged, modifiers }: { value: number; onValueChange?: (v: number) => void; onEditingChanged?: (editing: boolean) => void; modifiers?: { _label?: string }[] }) => {
      const labelMod = modifiers?.find((m) => m && m._label);
      const label = labelMod?._label;
      return React.createElement(View, {
        accessibilityRole: 'adjustable',
        accessibilityLabel: label,
        value,
        onChange: (v: number) => {
          onValueChange?.(v);
          onEditingChanged?.(false);
        },
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
            Pressable,
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
  disabled: (d: boolean = true) => ({ _disabled: d }),
  tint: (t: string) => ({ _tint: t }),
  labelsHidden: () => ({ _labelsHidden: true }),
}));

// @expo/ui jetpack-compose mocks for Jest.
jest.mock('@expo/ui/jetpack-compose', () => {
  const React = require('react');
  const { View, Text, TouchableOpacity } = require('react-native');
  return {
    Host: ({ children, style }: { children: React.ReactNode; style?: unknown }) => React.createElement(View, { style }, children),
    Text: ({ children, ...props }: { children?: React.ReactNode; [key: string]: unknown }) => React.createElement(Text, props, children),
    Switch: ({ value, onCheckedChange, modifiers }: { value: boolean; onCheckedChange?: (v: boolean) => void; modifiers?: { _label?: string }[] }) => {
      const labelMod = modifiers?.find((m) => m && m._label);
      return React.createElement(TouchableOpacity, {
        accessibilityRole: 'switch',
        accessibilityLabel: labelMod?._label,
        accessibilityState: { checked: value },
        onPress: () => onCheckedChange?.(!value),
      });
    },
    Slider: ({ value, onValueChange, onValueChangeFinished, modifiers }: { value: number; onValueChange?: (v: number) => void; onValueChangeFinished?: () => void; modifiers?: { _label?: string }[] }) => {
      const labelMod = modifiers?.find((m) => m && m._label);
      return React.createElement(View, {
        accessibilityRole: 'adjustable',
        accessibilityLabel: labelMod?._label,
        value,
        onChange: (v: number) => {
          onValueChange?.(v);
          onValueChangeFinished?.();
        },
      });
    },
    SegmentedButton: ({ children, ...props }: { children?: React.ReactNode; [key: string]: unknown }) => React.createElement(View, props, children),
    SingleChoiceSegmentedButtonRow: ({ children, ...props }: { children?: React.ReactNode; [key: string]: unknown }) => React.createElement(View, props, children),
  };
});

jest.mock('@expo/ui/jetpack-compose/modifiers', () => ({
  semantics: ({ contentDescription }: { contentDescription?: string }) => ({ _label: contentDescription }),
}));

// Gesture Handler's own Jest setup, FlashList setup, and safe areas without a native view.
require('react-native-gesture-handler/jestSetup');
require('@shopify/flash-list/jestSetup');
jest.mock('react-native-safe-area-context', () => require('react-native-safe-area-context/jest/mock').default);
jest.mock('react-native-keyboard-controller', () => require('react-native-keyboard-controller/jest'));
jest.mock('expo-router', () => ({
  router: {
    push: jest.fn(),
    replace: jest.fn(),
    back: jest.fn(),
    canGoBack: jest.fn(() => true),
  },
  useRouter: () => ({
    push: jest.fn(),
    replace: jest.fn(),
    back: jest.fn(),
    canGoBack: jest.fn(() => true),
  }),
  useLocalSearchParams: jest.fn(() => ({})),
  useGlobalSearchParams: jest.fn(() => ({})),
  useSegments: jest.fn(() => []),
  usePathname: jest.fn(() => '/'),
  useNavigation: jest.fn(() => ({
    setOptions: jest.fn(),
    addListener: jest.fn(() => () => {}),
  })),
  useFocusEffect: jest.fn((cb: () => void) => cb()),
}));

// The voice call's foreground service is native: its calls are recorded.
jest.mock('@opencord/voice-service', () => ({
  VoiceService: { start: jest.fn(), stop: jest.fn() },
}));
