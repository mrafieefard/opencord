// A call plays alongside other apps on iOS, as Discord's does. A session
// that doesn't mix is interrupted when another app starts playing, and the
// core's audio units then stop until the app is opened again.
export const VOICE_INTERRUPTION_MODE = 'mixWithOthers' as const;
