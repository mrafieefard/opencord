// Release builds signed with Opencord's key (docs/releasing.md, "Phone
// apps"). Expo's template signs them with the debug key, which no update can
// follow from a different machine. With OPENCORD_ANDROID_KEYSTORE naming a
// keystore, and ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS and
// ANDROID_KEY_PASSWORD its secrets, Gradle signs with that key; without
// them, with the debug key as before (CI then names the APK
// …-unsigned-debugkey.apk and the release leaves it out). The secrets are
// read when Gradle runs, so none of them ends up in the generated project.
import { type ConfigPlugin, withAppBuildGradle } from 'expo/config-plugins';

const MARK = 'opencordKeystore';

const RELEASE_CONFIG = `    signingConfigs {
        release {
            // Opencord's release key, when the environment names it (plugins/withReleaseSigning.ts).
            def ${MARK} = System.getenv('OPENCORD_ANDROID_KEYSTORE')
            if (${MARK}) {
                storeFile file(${MARK})
                storePassword System.getenv('ANDROID_KEYSTORE_PASSWORD')
                keyAlias System.getenv('ANDROID_KEY_ALIAS')
                keyPassword System.getenv('ANDROID_KEY_PASSWORD')
            }
        }
`;

/** Expo's app/build.gradle with release builds signed as above. */
export function signReleases(gradle: string): string {
  if (gradle.includes(MARK)) return gradle;
  const configs = '    signingConfigs {\n';
  // The release build type's own line, not the debug one's.
  const release = /(release \{[^}]*?)signingConfig signingConfigs\.debug/;
  if (!gradle.includes(configs) || !release.test(gradle)) {
    throw new Error("Expo's app/build.gradle has changed: plugins/withReleaseSigning.ts can't find its signing configs");
  }
  return gradle
    .replace(configs, RELEASE_CONFIG)
    .replace(release, `$1signingConfig System.getenv('OPENCORD_ANDROID_KEYSTORE') ? signingConfigs.release : signingConfigs.debug`);
}

export const withReleaseSigning: ConfigPlugin = (config) =>
  withAppBuildGradle(config, (mod) => {
    mod.modResults.contents = signReleases(mod.modResults.contents);
    return mod;
  });
