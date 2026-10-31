import { signReleases } from '../withReleaseSigning';

// The part of Expo's android/app/build.gradle the plugin changes, as
// `expo prebuild` writes it (SDK 58).
const TEMPLATE = `    signingConfigs {
        debug {
            storeFile file('debug.keystore')
            storePassword 'android'
            keyAlias 'androiddebugkey'
            keyPassword 'android'
        }
    }
    buildTypes {
        debug {
            signingConfig signingConfigs.debug
        }
        release {
            // Caution! In production, you need to generate your own keystore file.
            // see https://reactnative.dev/docs/signed-apk-android.
            signingConfig signingConfigs.debug
            def enableShrinkResources = findProperty('android.enableShrinkResourcesInReleaseBuilds') ?: 'false'
        }
    }
`;

describe('signReleases', () => {
  it('signs release builds with the key the environment names, else the debug key', () => {
    const gradle = signReleases(TEMPLATE);
    expect(gradle).toContain("def opencordKeystore = System.getenv('OPENCORD_ANDROID_KEYSTORE')");
    expect(gradle).toContain("storePassword System.getenv('ANDROID_KEYSTORE_PASSWORD')");
    expect(gradle).toContain("keyAlias System.getenv('ANDROID_KEY_ALIAS')");
    expect(gradle).toContain("keyPassword System.getenv('ANDROID_KEY_PASSWORD')");
    expect(gradle).toContain("signingConfig System.getenv('OPENCORD_ANDROID_KEYSTORE') ? signingConfigs.release : signingConfigs.debug");
    // Debug builds keep the debug key.
    expect(gradle).toMatch(/debug \{\n\s+signingConfig signingConfigs\.debug\n\s+\}/);
  });

  it('changes nothing the second time', () => {
    const once = signReleases(TEMPLATE);
    expect(signReleases(once)).toBe(once);
  });

  it("throws when Expo's template no longer looks as expected", () => {
    expect(() => signReleases('android {\n}\n')).toThrow(/build\.gradle/);
  });
});
