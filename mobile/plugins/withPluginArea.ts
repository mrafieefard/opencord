// Plugin areas' Flutter module (plugins M4, Task 4): the Android project
// finds the module's AARs, which mobile/scripts/build-flutter.sh builds into
// flutter_module/build/host/outputs/repo, and the Flutter engine they
// depend on, from Flutter's own repository (FLUTTER_STORAGE_BASE_URL for a
// mirror, as Flutter's documentation says).
import { type ConfigPlugin, withProjectBuildGradle } from 'expo/config-plugins';

const MARK = 'download.flutter.io';

const REPOSITORIES = `    // The plugin area's Flutter module and engine (plugins/withPluginArea.ts).
    maven { url "\${rootDir}/../flutter_module/build/host/outputs/repo" }
    maven { url "\${System.getenv('FLUTTER_STORAGE_BASE_URL') ?: 'https://storage.googleapis.com'}/${MARK}" }
`;

/** Expo's root build.gradle with both repositories in allprojects. */
export function addPluginAreaRepositories(gradle: string): string {
  if (gradle.includes(MARK)) return gradle;
  // The block's own closing brace is the first one indented by two spaces.
  const start = gradle.search(/allprojects \{\s*repositories \{/);
  const end = start < 0 ? -1 : gradle.indexOf('\n  }', start);
  if (end < 0) {
    throw new Error("Expo's android/build.gradle has changed: plugins/withPluginArea.ts can't find allprojects' repositories");
  }
  return `${gradle.slice(0, end)}\n${REPOSITORIES.trimEnd()}${gradle.slice(end)}`;
}

export const withPluginArea: ConfigPlugin = (config) =>
  withProjectBuildGradle(config, (mod) => {
    mod.modResults.contents = addPluginAreaRepositories(mod.modResults.contents);
    return mod;
  });
