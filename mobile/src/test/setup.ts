/// <reference types="jest" />
// Jest setup. @opencord/core-native maps to the generated bindings without
// their native installer (jest.config.js); tests use FakeCore for calls.

// ActionSheetIOS mock.
const { ActionSheetIOS } = require('react-native');
if (ActionSheetIOS) {
  ActionSheetIOS.showActionSheetWithOptions = jest.fn();
}

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
  PermissionStatus: {
    UNDETERMINED: 'undetermined',
    GRANTED: 'granted',
    DENIED: 'denied',
  },
  launchImageLibraryAsync: jest.fn(async () => ({ canceled: true, assets: [] })),
  launchCameraAsync: jest.fn(async () => ({ canceled: true, assets: [] })),
  requestCameraPermissionsAsync: jest.fn(async () => ({
    granted: true,
    status: 'granted',
    canAskAgain: true,
    expires: 'never',
  })),
  getCameraPermissionsAsync: jest.fn(async () => ({
    granted: true,
    status: 'granted',
    canAskAgain: true,
    expires: 'never',
  })),
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
class MockAudioPlayer {
  playing = false;
  currentTime = 0;
  duration = 30;
  volume = 1;
  listeners: ((status: unknown) => void)[] = [];

  play = jest.fn(() => {
    this.playing = true;
    this.emit();
  });
  pause = jest.fn(() => {
    this.playing = false;
    this.emit();
  });
  seekTo = jest.fn(async (seconds: number) => {
    this.currentTime = seconds;
    this.emit();
  });
  addListener = jest.fn((_event: string, listener: (status: unknown) => void) => {
    this.listeners.push(listener);
    return {
      remove: () => {
        this.listeners = this.listeners.filter((l) => l !== listener);
      },
    };
  });
  remove = jest.fn();
  source: unknown = null;
  replace = jest.fn((source: unknown) => {
    this.source = source;
  });

  emit() {
    for (const listener of this.listeners) {
      listener({
        currentTime: this.currentTime,
        duration: this.duration,
        playing: this.playing,
        isLoaded: true,
      });
    }
  }
}

export const mockAudioPlayer = new MockAudioPlayer();

jest.mock('expo-audio', () => ({
  getRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestNotificationPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  setAudioModeAsync: jest.fn(async () => {}),
  setIsAudioActiveAsync: jest.fn(async () => {}),
  createAudioPlayer: jest.fn(() => mockAudioPlayer),
  useAudioPlayer: jest.fn(() => mockAudioPlayer),
  useAudioPlayerStatus: jest.fn((player: { currentTime?: number; duration?: number; playing?: boolean } | null | undefined) => ({
    currentTime: player?.currentTime ?? 0,
    duration: player?.duration ?? 30,
    playing: player?.playing ?? false,
    isLoaded: true,
    isBuffering: false,
    playbackRate: 1,
    muted: false,
    loop: false,
    didJustFinish: false,
  })),
}));

// expo-video mock.
class MockVideoPlayer {
  playing = false;
  currentTime = 0;
  duration = 60;
  volume = 1;
  play = jest.fn(() => {
    this.playing = true;
  });
  pause = jest.fn(() => {
    this.playing = false;
  });
  seekBy = jest.fn();
  replay = jest.fn();
}

export const mockVideoPlayer = new MockVideoPlayer();

jest.mock('expo-video', () => {
  const React = require('react');
  const { View } = require('react-native');
  return {
    VideoView: jest.fn(({ player: _player, style, testID, ...rest }: { player?: unknown; style?: unknown; testID?: string; [key: string]: unknown }) => {
      return React.createElement(View, {
        testID: testID ?? 'expo-video-view',
        style,
        accessibilityLabel: 'Video player',
        ...rest,
      });
    }),
    useVideoPlayer: jest.fn((_source: unknown, setup?: (p: MockVideoPlayer) => void) => {
      const p = new MockVideoPlayer();
      if (setup) setup(p);
      return p;
    }),
    createVideoPlayer: jest.fn(() => new MockVideoPlayer()),
    isPictureInPictureSupported: jest.fn(() => true),
  };
});

// @expo/ui swift-ui mocks for Jest.
jest.mock('@expo/ui/swift-ui', () => {
  const React = require('react');
  const { View, Text, Pressable } = require('react-native');
  // A context menu shows its items beside its trigger; a button among them
  // is a menu item.
  const InMenu = React.createContext(false);
  const ContextMenu = ({ children }: { children: React.ReactNode }) => React.createElement(View, null, children);
  ContextMenu.Items = function Items({ children }: { children: React.ReactNode }) {
    return React.createElement(InMenu.Provider, { value: true }, React.createElement(View, null, children));
  };
  ContextMenu.Trigger = function Trigger({ children }: { children: React.ReactNode }) {
    return React.createElement(React.Fragment, null, children);
  };
  // A row takes its tap through its onTapGesture modifier; its swipe
  // actions sit beside it.
  const SwipeActions = ({ children, testID, modifiers }: { children: React.ReactNode; testID?: string; modifiers?: { _onTapGesture?: () => void }[] }) =>
    React.createElement(Pressable, { testID, onPress: modifiers?.find((m) => m && m._onTapGesture)?._onTapGesture }, children);
  SwipeActions.Actions = function Actions({ children }: { children: React.ReactNode }) {
    return React.createElement(View, null, children);
  };
  return {
    // Laid out at a phone's width as soon as it mounts.
    Host: function Host({ children, style, testID, onLayoutContent }: { children: React.ReactNode; style?: unknown; testID?: string; onLayoutContent?: (event: { nativeEvent: { width: number; height: number } }) => void }) {
      React.useEffect(() => onLayoutContent?.({ nativeEvent: { width: 390, height: 844 } }), [onLayoutContent]);
      return React.createElement(View, { style, testID }, children);
    },
    // A list refreshes through its refreshable modifier.
    List: ({ children, modifiers }: { children?: React.ReactNode; modifiers?: { _refreshable?: () => Promise<void> }[] }) =>
      React.createElement(View, { accessible: true, accessibilityRole: 'list', onRefresh: modifiers?.find((m) => m && m._refreshable)?._refreshable }, children),
    SwipeActions,
    ContextMenu,
    RNHostView: ({ children }: { children: React.ReactNode }) => children,
    Group: ({ children }: { children: React.ReactNode }) => React.createElement(View, null, children),
    // Named by its label, or by accessibilityLabel when it draws its own content.
    Button: function Button({ label, role, onPress, modifiers, testID, children }: { label?: string; role?: string; onPress?: () => void; modifiers?: { _disabled?: boolean; _label?: string }[]; testID?: string; children?: React.ReactNode }) {
      const inMenu = React.useContext(InMenu);
      return React.createElement(
        Pressable,
        {
          testID,
          accessibilityRole: inMenu ? 'menuitem' : 'button',
          accessibilityLabel: modifiers?.find((m) => m && m._label)?._label ?? label,
          accessibilityHint: role === 'destructive' ? 'destructive' : undefined,
          disabled: modifiers?.some((m) => m && m._disabled) ?? false,
          onPress,
        },
        children,
      );
    },
    HStack: ({ children }: { children?: React.ReactNode }) => React.createElement(View, null, children),
    Spacer: () => null,
    // A menu button: named by its label (or accessibilityLabel), its items
    // beside it as menu items.
    Menu: ({ label, modifiers, testID, children }: { label?: React.ReactNode; modifiers?: { _label?: string }[]; testID?: string; children?: React.ReactNode }) =>
      React.createElement(
        View,
        { testID, accessibilityRole: 'button', accessibilityLabel: modifiers?.find((m) => m && m._label)?._label ?? (typeof label === 'string' ? label : undefined) },
        React.createElement(InMenu.Provider, { value: true }, children),
      ),
    Image: () => null,
    ProgressView: () => React.createElement(View, { accessibilityRole: 'progressbar' }),
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
  buttonStyle: (style: string) => ({ _buttonStyle: style }),
  buttonBorderShape: () => ({}),
  labelStyle: () => ({}),
  controlSize: () => ({}),
  font: () => ({}),
  foregroundStyle: () => ({}),
  frame: () => ({}),
  padding: () => ({}),
  glassEffect: () => ({}),
  background: () => ({}),
  shapes: { capsule: () => ({}) },
  pickerStyle: () => ({ _style: 'segmented' }),
  tag: (idx: number) => ({ _tag: idx }),
  accessibilityLabel: (lbl: string) => ({ _label: lbl }),
  disabled: (d: boolean = true) => ({ _disabled: d }),
  tint: (t: string) => ({ _tint: t }),
  labelsHidden: () => ({ _labelsHidden: true }),
  listRowInsets: () => ({}),
  listRowBackground: () => ({}),
  listRowSeparator: () => ({}),
  listStyle: () => ({}),
  scrollContentBackground: () => ({}),
  refreshable: (action: () => Promise<void>) => ({ _refreshable: action }),
  onTapGesture: (action: () => void) => ({ _onTapGesture: action }),
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
jest.mock('@opencord/voice-service', () => {
  const React = require('react');
  const { View } = require('react-native');
  return {
    VoiceService: { start: jest.fn(), stop: jest.fn() },
    AudioRouteButton: ({ testID }: { testID?: string }) =>
      React.createElement(View, { testID, accessible: true, accessibilityRole: 'button', accessibilityLabel: 'Audio output' }),
    hasAudioRouteButton: true,
  };
});

// Liquid Glass is native: a plain view stands in.
jest.mock('expo-glass-effect', () => {
  const React = require('react');
  const { View } = require('react-native');
  return {
    GlassView: ({ children, style, testID, pointerEvents }: { children?: React.ReactNode; style?: unknown; testID?: string; pointerEvents?: string }) =>
      React.createElement(View, { style, testID, pointerEvents }, children),
  };
});
