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

const Animated = {
  View,
  Text,
  ScrollView,
  FlatList,
  createAnimatedComponent,
};
export default Animated;

export function useSharedValue<T>(value: T): { value: T; get: () => T; set: (next: T) => void } {
  const shared = { value, get: () => shared.value, set: (next: T) => void (shared.value = next) };
  return shared;
}

export function useAnimatedStyle<T>(make: () => T): T {
  return make();
}

export function useDerivedValue<T>(make: () => T): { value: T } {
  return { value: make() };
}

export const withTiming = <T,>(value: T) => value;
export const withSpring = <T,>(value: T) => value;
export const withDelay = <T,>(_ms: number, value: T) => value;
export const useReducedMotion = () => false;
export const cancelAnimation = () => {};
export const interpolate = (value: number, input: number[], output: number[]) => {
  const [inStart = 0, inEnd = 1] = input;
  const [outStart = 0, outEnd = 1] = output;
  return outStart + ((value - inStart) / (inEnd - inStart || 1)) * (outEnd - outStart);
};
export const Extrapolation = { CLAMP: 'clamp', EXTEND: 'extend', IDENTITY: 'identity' } as const;
export const ReduceMotion = { System: 'system', Always: 'always', Never: 'never' } as const;
