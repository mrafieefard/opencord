// The voice call's foreground service (design §11). On Android it keeps the
// process, the microphone and playback alive while the app is in the
// background, under an ongoing notification that opens the app. On iOS both
// calls do nothing: the `audio` background mode keeps a call going.
import { NativeModule, requireNativeModule } from 'expo';

export interface VoiceNotification {
  title: string;
  /** A second line; empty for none. */
  text: string;
}

declare class VoiceServiceModule extends NativeModule {
  start(title: string, text: string): void;
  stop(): void;
}

const native = requireNativeModule<VoiceServiceModule>('VoiceService');

export const VoiceService = {
  /** Starts the service, or changes its notification when it runs already. */
  start({ title, text }: VoiceNotification): void {
    native.start(title, text);
  },
  stop(): void {
    native.stop();
  },
};

export { AudioRouteButton, hasAudioRouteButton } from './AudioRouteButton';
export type { AudioRouteButtonProps } from './AudioRouteButton.types';
