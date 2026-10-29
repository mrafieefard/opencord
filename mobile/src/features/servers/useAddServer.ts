// State machine for adding a server, handling trust steps and claim tokens.
import { useCallback, useState } from 'react';
import * as Clipboard from 'expo-clipboard';
import { useRouter } from 'expo-router';
import { AddServerResult } from '@opencord/core-native';

import { describeError, report } from '@/core';
import { addServer, trustServer } from '@/state/actions/servers';

export interface UseAddServerOptions {
  initialLink?: string;
  onSuccess?: () => void;
}

export function useAddServer(options?: UseAddServerOptions) {
  const router = useRouter();
  const [linkOrAddress, setLinkOrAddress] = useState(options?.initialLink ?? '');
  const [claimToken, setClaimToken] = useState('');
  const [showClaimToken, setShowClaimToken] = useState(false);
  const [trustPending, setTrustPending] = useState<{ address: string; fingerprint: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const paste = useCallback(() => {
    report(
      Clipboard.getStringAsync().then((text) => {
        if (!text) return;
        setLinkOrAddress(text.trim());
        setError(null);
      }),
      'paste',
    );
  }, []);

  const handleAdded = useCallback(() => {
    if (options?.onSuccess) {
      options.onSuccess();
    } else {
      router.back();
    }
  }, [options, router]);

  const submit = useCallback(async () => {
    const target = linkOrAddress.trim();
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      const token = claimToken.trim() || undefined;
      const result = await addServer(target, token);
      if (AddServerResult.NeedsTrust.instanceOf(result)) {
        setTrustPending({
          address: result.inner.address,
          fingerprint: result.inner.fingerprint,
        });
      } else if (AddServerResult.Added.instanceOf(result)) {
        handleAdded();
      }
    } catch (failure: unknown) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  }, [linkOrAddress, claimToken, handleAdded]);

  const confirmTrust = useCallback(async () => {
    if (!trustPending) return;
    setBusy(true);
    setError(null);
    try {
      trustServer(trustPending.address, trustPending.fingerprint);
      const token = claimToken.trim() || undefined;
      const result = await addServer(linkOrAddress.trim(), token);
      if (AddServerResult.Added.instanceOf(result)) {
        handleAdded();
      } else {
        // Asked again: the certificate changed between the two attempts.
        setTrustPending({ address: result.inner.address, fingerprint: result.inner.fingerprint });
      }
    } catch (failure: unknown) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  }, [trustPending, claimToken, linkOrAddress, handleAdded]);

  const cancelTrust = useCallback(() => {
    setTrustPending(null);
    setError(null);
  }, []);

  // A new address makes the last error stale.
  const editLinkOrAddress = useCallback((text: string) => {
    setLinkOrAddress(text);
    setError(null);
  }, []);

  return {
    linkOrAddress,
    setLinkOrAddress: editLinkOrAddress,
    claimToken,
    setClaimToken,
    showClaimToken,
    setShowClaimToken,
    trustPending,
    error,
    busy,
    paste,
    submit,
    confirmTrust,
    cancelTrust,
  };
}
