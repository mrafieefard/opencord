import {
  usePanGesture,
  usePinchGesture,
  useTapGesture,
  useSimultaneousGestures,
  useExclusiveGestures,
} from 'react-native-gesture-handler';
import {
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';

export interface UseZoomPanOptions {
  onClose: () => void;
  onToggleControls?: () => void;
}

export function useZoomPan({ onClose, onToggleControls }: UseZoomPanOptions) {
  const scale = useSharedValue(1);
  const savedScale = useSharedValue(1);
  const translateX = useSharedValue(0);
  const translateY = useSharedValue(0);
  const savedTranslateX = useSharedValue(0);
  const savedTranslateY = useSharedValue(0);

  const doubleTap = useTapGesture({
    numberOfTaps: 2,
    maxDuration: 250,
    onDeactivate: () => {
      if (scale.get() > 1.2) {
        scale.set(withTiming(1));
        savedScale.set(1);
        translateX.set(withTiming(0));
        translateY.set(withTiming(0));
        savedTranslateX.set(0);
        savedTranslateY.set(0);
      } else {
        scale.set(withTiming(2.5));
        savedScale.set(2.5);
      }
    },
  });

  const singleTap = useTapGesture({
    numberOfTaps: 1,
    maxDistance: 10,
    onDeactivate: () => {
      if (onToggleControls) {
        scheduleOnRN(onToggleControls);
      }
    },
  });

  const taps = useExclusiveGestures(doubleTap, singleTap);

  const pinch = usePinchGesture({
    onUpdate: (e) => {
      scale.set(Math.max(1, Math.min(savedScale.get() * e.scale, 5)));
    },
    onDeactivate: () => {
      savedScale.set(scale.get());
      if (scale.get() <= 1) {
        translateX.set(withTiming(0));
        translateY.set(withTiming(0));
        savedTranslateX.set(0);
        savedTranslateY.set(0);
      }
    },
  });

  const pan = usePanGesture({
    testID: 'viewer-pan',
    activeOffsetY: [-10, 10],
    onUpdate: (e) => {
      if (scale.get() > 1.05) {
        translateX.set(savedTranslateX.get() + e.translationX);
        translateY.set(savedTranslateY.get() + e.translationY);
      } else {
        if (e.translationY > 0) {
          translateY.set(e.translationY);
        }
      }
    },
    onDeactivate: (e) => {
      if (scale.get() > 1.05) {
        savedTranslateX.set(translateX.get());
        savedTranslateY.set(translateY.get());
      } else {
        if (e.translationY > 100 || (e.translationY > 30 && e.velocityY > 600)) {
          scheduleOnRN(onClose);
        } else {
          translateY.set(withSpring(0));
        }
      }
    },
  });

  const gesture = useSimultaneousGestures(taps, pinch, pan);

  const imageAnimatedStyle = useAnimatedStyle(() => ({
    transform: [
      { translateX: translateX.get() },
      { translateY: translateY.get() },
      { scale: scale.get() },
    ],
  }));

  const backdropAnimatedStyle = useAnimatedStyle(() => {
    const s = scale.get();
    const ty = translateY.get();
    if (s <= 1.05 && ty > 0) {
      const opacity = Math.max(0, 1 - ty / 300);
      return { opacity };
    }
    return { opacity: 1 };
  });

  return {
    gesture,
    imageAnimatedStyle,
    backdropAnimatedStyle,
    scale,
  };
}
