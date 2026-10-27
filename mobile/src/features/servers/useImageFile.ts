// Caches avatarFile paths as file:// URIs (spec §8.2, plan Task 8).
import { useEffect, useState } from 'react';

import { getCore } from '@/core';
import { fileUri } from '@/core/paths';

const MAX_CACHE_ENTRIES = 200;
const pathCache = new Map<string, string>();

function cacheKey(serverKey: string, hash: string): string {
  return `${serverKey}:${hash}`;
}

export function useImageFile(serverKey: string, hash?: string | null): string | null {
  const key = hash ? cacheKey(serverKey, hash) : null;
  const cached = key ? pathCache.get(key) ?? null : null;
  const [file, setFile] = useState<string | null>(cached);

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
        setFile(uri);
      })
      .catch(() => {
        if (!active) return;
        setFile(null);
      });

    return () => {
      active = false;
    };
  }, [serverKey, hash, key]);

  return hash ? (cached ?? file) : null;
}
