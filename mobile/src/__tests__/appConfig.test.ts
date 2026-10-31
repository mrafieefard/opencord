import config from '../../app.config';

function pluginOptions(name: string): unknown {
  const entry = (config.plugins ?? []).find((candidate) => (Array.isArray(candidate) ? candidate[0] : candidate) === name);
  return Array.isArray(entry) && entry.length === 2 ? entry[1] : undefined;
}

describe('the app config', () => {
  it('no plugin blocks the microphone voice needs', () => {
    // expo-image-picker's `microphonePermission: false` removes RECORD_AUDIO
    // from the manifest and NSMicrophoneUsageDescription from Info.plist.
    const picker = pluginOptions('expo-image-picker');
    expect(picker).toBeDefined();
    expect(picker).not.toMatchObject({ microphonePermission: false });
    expect(config.android?.permissions).toContain('android.permission.RECORD_AUDIO');
    expect(config.ios?.infoPlist?.NSMicrophoneUsageDescription).toBe('Opencord uses the microphone in voice channels.');
  });
});
