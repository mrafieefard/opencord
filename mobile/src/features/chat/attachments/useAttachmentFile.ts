import { useEffect, useState } from 'react';
import { AttachmentVariant } from '@opencord/core-native';

import { describeError, getCore } from '@/core';
import { fileUri } from '@/core/paths';

const MAX_CACHE_ENTRIES = 200;
const pathCache = new Map<string, string>();
const inFlight = new Map<string, Promise<string>>();

export function attachmentKey(serverKey: string, attachmentId: string, variant: AttachmentVariant): string {
  return `${serverKey}:${attachmentId}:${variant}`;
}

/**
 * Downloads or retrieves the cached path of an attachment.
 * Deduplicates in-flight downloads so multiple callers share one promise.
 */
export function fetchAttachmentFile(
  serverKey: string,
  attachmentId: string,
  variant: AttachmentVariant,
): Promise<string> {
  const key = attachmentKey(serverKey, attachmentId, variant);
  const cached = pathCache.get(key);
  if (cached !== undefined) return Promise.resolve(cached);

  const existing = inFlight.get(key);
  if (existing !== undefined) return existing;

  const promise = getCore()
    .attachmentFile(serverKey, attachmentId, variant)
    .then((rawPath) => {
      const uri = fileUri(rawPath);
      if (pathCache.size >= MAX_CACHE_ENTRIES) {
        const oldest = pathCache.keys().next().value;
        if (oldest !== undefined) pathCache.delete(oldest);
      }
      pathCache.set(key, uri);
      inFlight.delete(key);
      return uri;
    })
    .catch((err: unknown) => {
      inFlight.delete(key);
      throw err;
    });

  inFlight.set(key, promise);
  return promise;
}

export interface AttachmentFileResult {
  path?: string;
  loading: boolean;
  error?: string;
}

export function useAttachmentFile(
  serverKey: string,
  attachmentId?: string,
  variant: AttachmentVariant = AttachmentVariant.Thumbnail,
): AttachmentFileResult {
  const key = attachmentId ? attachmentKey(serverKey, attachmentId, variant) : null;
  const cached = key ? pathCache.get(key) : undefined;

  const [state, setState] = useState<{ key: string | null; path?: string; error?: string }>(() => ({
    key,
    path: cached,
    error: undefined,
  }));

  useEffect(() => {
    if (!key || !attachmentId) {
      return;
    }

    if (pathCache.has(key)) {
      return;
    }

    let active = true;

    fetchAttachmentFile(serverKey, attachmentId, variant)
      .then((path) => {
        if (!active) return;
        setState({ key, path, error: undefined });
      })
      .catch((err: unknown) => {
        if (!active) return;
        setState({ key, path: undefined, error: describeError(err) });
      });

    return () => {
      active = false;
    };
  }, [serverKey, attachmentId, variant, key]);

  if (!key) return { loading: false };
  if (cached !== undefined) return { path: cached, loading: false };
  if (state.key === key) {
    if (state.error) return { loading: false, error: state.error };
    if (state.path) return { path: state.path, loading: false };
  }
  return { loading: true };
}
