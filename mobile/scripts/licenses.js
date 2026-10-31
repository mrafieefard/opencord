#!/usr/bin/env node
// Fails on any production dependency whose licence isn't allowed (plan:
// Global Constraints). Walks the app's and its workspaces' dependencies
// through node_modules, as Node resolves them.

const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const allowed = new Set(['MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Zlib', '0BSD']);
// MPL-2.0 only for UniFFI's bindings and their runtime, and our own code.
const mplAllowed = new Set(['uniffi-bindgen-react-native', '@ubjs/core', 'opencord-mobile']);

// Already in Expo SDK 58's own tree (its bundler, CSS and glob tooling),
// all permissive; listed by name so nothing new slips in under them.
const fromExpo = new Map([
  ['argparse', 'Python-2.0'],
  ['big-integer', 'Unlicense'],
  ['caniuse-lite', 'CC-BY-4.0'],
  ['glob', 'BlueOak-1.0.0'],
  ['lightningcss', 'MPL-2.0'],
  ['lightningcss-linux-x64-gnu', 'MPL-2.0'],
  ['lightningcss-linux-x64-musl', 'MPL-2.0'],
  ['lightningcss-darwin-arm64', 'MPL-2.0'],
  ['lightningcss-darwin-x64', 'MPL-2.0'],
  ['lru-cache', 'BlueOak-1.0.0'],
  ['minimatch', 'BlueOak-1.0.0'],
  ['minipass', 'BlueOak-1.0.0'],
  ['path-scurry', 'BlueOak-1.0.0'],
  ['sax', 'BlueOak-1.0.0'],
  ['stream-buffers', 'Unlicense'],
]);

function licenseOf(pkg) {
  if (typeof pkg.license === 'string') return pkg.license;
  if (pkg.license && typeof pkg.license.type === 'string') return pkg.license.type;
  if (Array.isArray(pkg.licenses)) {
    return pkg.licenses.map((entry) => entry.type ?? entry).join(' OR ');
  }
  return 'UNKNOWN';
}

function acceptable(name, license) {
  if (fromExpo.get(name) === license) return true;
  const ok = (id) => allowed.has(id) || (id === 'MPL-2.0' && (mplAllowed.has(name) || name.startsWith('@opencord/')));
  const expression = license.replace(/[()]/g, ' ').trim();
  return expression
    .split(/\s+OR\s+/)
    .some((alternative) => alternative.split(/\s+AND\s+/).every((id) => ok(id.trim())));
}

/** The folder of `name` as Node finds it from `from`. */
function resolve(name, from) {
  let dir = from;
  for (;;) {
    const candidate = path.join(dir, 'node_modules', name, 'package.json');
    if (fs.existsSync(candidate)) return path.dirname(candidate);
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

const seen = new Set();
const refused = [];
const missing = [];

function visit(dir) {
  const real = fs.realpathSync(dir);
  if (seen.has(real)) return;
  seen.add(real);
  const pkg = JSON.parse(fs.readFileSync(path.join(real, 'package.json'), 'utf8'));
  const license = licenseOf(pkg);
  if (!acceptable(pkg.name, license)) refused.push(`${pkg.name}@${pkg.version}: ${license}`);
  const deps = { ...pkg.dependencies, ...pkg.optionalDependencies };
  for (const name of Object.keys(deps)) {
    const found = resolve(name, real);
    if (found) {
      visit(found);
    } else if (!(pkg.optionalDependencies && name in pkg.optionalDependencies)) {
      missing.push(`${name} (needed by ${pkg.name})`);
    }
  }
}

visit(root);
for (const group of ['packages', 'modules']) {
  const folder = path.join(root, group);
  if (!fs.existsSync(folder)) continue;
  for (const entry of fs.readdirSync(folder)) {
    if (fs.existsSync(path.join(folder, entry, 'package.json'))) visit(path.join(folder, entry));
  }
}

if (missing.length) {
  console.error(`Not installed (run yarn install):\n  ${missing.join('\n  ')}`);
  process.exit(1);
}
if (refused.length) {
  console.error(`Licences not on the list:\n  ${refused.sort().join('\n  ')}`);
  process.exit(1);
}
console.log(`${seen.size} production packages, every licence allowed.`);
