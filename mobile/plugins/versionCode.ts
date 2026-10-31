// Android's version code for a release version (docs/releasing.md). Android
// installs an update only over a lower code, so the codes keep SemVer's order,
// beta < canary < stable, as releases go out (beta, canary, stable):
//
//   major * 10 000 000 + minor * 100 000 + patch * 1 000 + rank * 100 + pre
//
// rank is 0 for beta, 1 for canary and 9 for stable; pre is the pre-release
// number (0 for stable).

const RANK: Record<string, number> = { beta: 0, canary: 1 };
const STABLE_RANK = 9;
/** Google Play's largest version code. */
const LARGEST = 2_100_000_000;

export function versionCode(version: string): number {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-(beta|canary)\.(\d+))?$/.exec(version);
  if (!match) throw new Error(`${version} is not a release version`);
  const [major, minor, patch] = [match[1], match[2], match[3]].map(Number) as [number, number, number];
  const channel = match[4];
  const pre = channel ? Number(match[5]) : 0;
  for (const [name, value] of [['minor', minor], ['patch', patch], ['pre-release number', pre]] as const) {
    if (value > 99) throw new Error(`${version}: a ${name} over 99 has no room in the version code`);
  }
  const rank = channel ? (RANK[channel] ?? STABLE_RANK) : STABLE_RANK;
  const code = major * 10_000_000 + minor * 100_000 + patch * 1_000 + rank * 100 + pre;
  if (code > LARGEST) throw new Error(`${version} makes version code ${code}, over Google Play's ${LARGEST}`);
  return code;
}
