// Android has no session to turn on: the core's streams open as they are,
// and the call's audio focus comes with its audio mode (audioActivation.ios.ts).
export async function activateAudio(): Promise<void> {}

export async function deactivateAudio(): Promise<void> {}
