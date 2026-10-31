// The call's sound output on iOS: the glyph of where it goes now, opening
// the system's route menu (speaker, iPhone, AirPods…).
import { requireNativeView } from 'expo';
import type { AudioRouteButtonProps } from './AudioRouteButton.types';

const NativeAudioRouteButton = requireNativeView<AudioRouteButtonProps>('VoiceService', 'AudioRouteButtonView');

export function AudioRouteButton(props: AudioRouteButtonProps) {
  return <NativeAudioRouteButton {...props} />;
}

/** Whether this platform has the button. */
export const hasAudioRouteButton = true;
