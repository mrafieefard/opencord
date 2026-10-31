// Caches avatarFile paths as file:// URIs (spec §8.2, plan Task 8).
import { useEffect, useState } from 'react';

import { errorKind, getCore } from '@/core';
import { fileUri } from '@/core/paths';

const MAX_CACHE_ENTRIES = 200;
const pathCache = new Map<string, string>();

function cacheKey(serverKey: string, hash: string): string {
  return `${serverKey}:${hash}`;
}

export function useImageFile(serverKey: string, hash?: string | null): string | null {
  const key = hash ? cacheKey(serverKey, hash) : null;
  const cached = key ? pathCache.get(key) ?? null : null;
  // Tagged with its key, so a new hash never shows the old picture.
  const [loaded, setLoaded] = useState<{ key: string; uri: string } | null>(null);

  useEffect(() => {
    if (!key || !hash) return;
    if (pathCache.has(key)) return;

    let active = true;
    getCore()
      .avatarFile(serverKey, hash)
      .then((path) => {
        if (!active) return;
        const uri = fileUri(path);
        if (pathCache.size >= MAX_CACHE_ENTRIES) {
          const oldestKey = pathCache.keys().next().value;
          if (oldestKey !== undefined) pathCache.delete(oldestKey);
        }
        pathCache.set(key, uri);
        setLoaded({ key, uri });
      })
      .catch((error: unknown) => {
        console.warn(`picture ${hash} failed: ${errorKind(error)}`);
      });

    return () => {
      active = false;
    };
  }, [serverKey, hash, key]);

  return cached ?? (loaded !== null && loaded.key === key ? loaded.uri : null);
}
