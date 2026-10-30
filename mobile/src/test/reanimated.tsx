// The part of Reanimated the app uses, for Jest: views are plain views,
// values change at once, layout animations do nothing. Grows with the app.
import type { ComponentType } from 'react';
import { FlatList, ScrollView, Text, View } from 'react-native';

type Builder = { duration: (ms: number) => Builder; delay: (ms: number) => Builder; springify: () => Builder };
function builder(): Builder {
  const self: Builder = { duration: () => self, delay: () => self, springify: () => self };
  return self;
}

export const FadeIn = builder();
export const FadeOut = builder();
export const FadeInDown = builder();
export const FadeOutDown = builder();
export const ZoomIn = builder();
export const ZoomOut = builder();
export const SlideInRight = builder();
export const SlideOutRight = builder();
export const LinearTransition = builder();

/** React 19 passes `ref` as a prop, so the component stands in as it is. */
export function createAnimatedComponent<P extends object>(component: ComponentType<P>): ComponentType<P> {
  return component;
}

export function runOnJS<T extends (...args: unknown[]) => unknown>(fn: T): T {
  return fn;
}

export function useEvent(callback: unknown, _eventNames?: string[], _rebuild?: boolean) {
  return callback;
}

export function useComposedEventHandler(_handlers: unknown[]) {
  return null;
}

const Animated = {
  View,
  Text,
  ScrollView,
  FlatList,
  createAnimatedComponent,
  isSharedValue,
  runOnJS,
  useEvent,
  useComposedEventHandler,
};
export default Animated;

export function isSharedValue(value: unknown): boolean {
  return typeof value === 'object' && value !== null && 'value' in value;
}

export function useAnimatedRef<T = unknown>() {
  return { current: null as T | null };
}

export function measure(_ref: unknown) {
  return { x: 0, y: 0, width: 0, height: 0, pageX: 0, pageY: 0 };
}

export function useSharedValue<T>(value: T): {
  value: T;
  get: () => T;
  set: (next: T) => void;
  addListener: (id: number, listener: (value: T) => void) => void;
  removeListener: (id: number) => void;
  modify: (modifier: (value: T) => T) => void;
} {
  const listeners = new Map<number, (value: T) => void>();
  const shared = {
    value,
    get: () => shared.value,
    set: (next: T) => {
      shared.value = next;
      for (const listener of listeners.values()) listener(next);
    },
    addListener: (id: number, listener: (value: T) => void) => {
      listeners.set(id, listener);
    },
    removeListener: (id: number) => {
      listeners.delete(id);
    },
    modify: (modifier: (value: T) => T) => {
      shared.set(modifier(shared.value));
    },
  };
  return shared;
}

export function useAnimatedStyle<T extends object>(make: () => T): T {
  const style = make();
  if (style && typeof style === 'object' && 'pointerEvents' in style && (style as { pointerEvents?: string }).pointerEvents === 'none') {
    return { ...style, pointerEvents: 'auto' };
  }
  return style;
}

export function useDerivedValue<T>(make: () => T): { value: T } {
  return { value: make() };
}

export const withTiming = <T,>(value: T) => value;
export const withSpring = <T,>(value: T) => value;
export const withDelay = <T,>(_ms: number, value: T) => value;
export const withRepeat = <T,>(value: T) => value;
export const makeMutable = useSharedValue;
export const useAnimatedReaction = () => {};
export const useReducedMotion = () => false;
export const cancelAnimation = () => {};
export const interpolate = (value: number, input: number[], output: number[]) => {
  const [inStart = 0, inEnd = 1] = input;
  const [outStart = 0, outEnd = 1] = output;
  return outStart + ((value - inStart) / (inEnd - inStart || 1)) * (outEnd - outStart);
};
export const Extrapolation = { CLAMP: 'clamp', EXTEND: 'extend', IDENTITY: 'identity' } as const;
export const ReduceMotion = { System: 'system', Always: 'always', Never: 'never' } as const;
export const Easing = { linear: (t: number) => t };
