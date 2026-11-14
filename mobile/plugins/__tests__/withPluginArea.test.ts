import { addPluginAreaRepositories } from '../withPluginArea';

// The root android/build.gradle as `expo prebuild` writes it (SDK 58).
const TEMPLATE = `allprojects {
  repositories {
    google()
    mavenCentral()
    maven { url 'https://www.jitpack.io' }
  }
}

apply plugin: "expo-root-project"
`;

describe('withPluginArea', () => {
  // Fails if either repository is missing, or a second prebuild adds them
  // again.
  it('adds both repositories exactly once when applied twice', () => {
    const once = addPluginAreaRepositories(TEMPLATE);
    const twice = addPluginAreaRepositories(once);
    expect(twice).toBe(once);
    expect(once.match(/flutter_module\/build\/host\/outputs\/repo/g)).toHaveLength(1);
    expect(once.match(/download\.flutter\.io/g)).toHaveLength(1);
    // Inside allprojects' repositories, after the template's own.
    expect(once.indexOf('jitpack')).toBeLessThan(once.indexOf('download.flutter.io'));
    expect(once.indexOf('download.flutter.io')).toBeLessThan(once.indexOf('apply plugin'));
  });

  // Fails if a changed template is silently left without the repositories.
  it('throws when the template has changed', () => {
    expect(() => addPluginAreaRepositories('buildscript {}\n')).toThrow(/withPluginArea/);
  });
});
