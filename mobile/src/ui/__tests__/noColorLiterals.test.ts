/// <reference types="node" />
import { readdirSync, readFileSync, statSync } from 'fs';
import { join, resolve } from 'path';

const root = resolve(__dirname, '../..');
const allowed = [join('ui', 'theme', 'tokens.ts')];

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === '__tests__' ? [] : files(path);
    return /\.(ts|tsx)$/.test(name) ? [path] : [];
  });
}

describe('the colors', () => {
  it('no color literal outside tokens.ts', () => {
    const found = files(root)
      .filter((path) => !allowed.some((suffix) => path.endsWith(suffix)))
      .flatMap((path) =>
        readFileSync(path, 'utf8')
          .split('\n')
          .flatMap((line, index) =>
            /#[0-9a-fA-F]{3,8}\b|rgba?\(/.test(line) ? [`${path.slice(root.length + 1)}:${index + 1}: ${line.trim()}`] : [],
          ),
      );
    expect(found).toEqual([]);
  });
});
