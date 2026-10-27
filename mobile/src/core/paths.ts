// Where the core keeps its data and its attachment files, as plain paths.
import { Paths } from 'expo-file-system';

function plain(uri: string): string {
  const path = decodeURI(uri.replace(/^file:\/\//, ''));
  return path.endsWith('/') ? path.slice(0, -1) : path;
}

export function dataDir(): string {
  return `${plain(Paths.document.uri)}/opencord`;
}

export function cacheDir(): string {
  return `${plain(Paths.cache.uri)}/opencord`;
}

export function fileUri(path: string): string {
  if (path.startsWith('file://')) return path;
  return `file://${path.startsWith('/') ? '' : '/'}${path}`;
}
