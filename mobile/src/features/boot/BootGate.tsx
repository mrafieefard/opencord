// Opens the core once, then lets the routes show. The splash screen stays
// until the answer is in.
import * as SplashScreen from 'expo-splash-screen';
import { useCallback, useEffect, type ReactNode } from 'react';

import { bootCore, bootDeps, getCore } from '@/core';
import { startApp } from '@/core/lifecycle';
import { useBoot } from '@/state/selectors';
import { useAppStore } from '@/state/store';

import { BootScreen } from './BootScreen';

void SplashScreen.preventAutoHideAsync().catch(() => {});

export async function boot(): Promise<void> {
  useAppStore.setState({ boot: { state: 'starting' } });
  const result = await bootCore(getCore(), bootDeps());
  if (result.state === 'ready') {
    useAppStore.setState({ boot: { state: 'ready' }, self: result.self });
    startApp();
  } else if (result.state === 'onboarding') {
    useAppStore.setState({ boot: { state: 'onboarding' } });
  } else {
    useAppStore.setState({ boot: { state: 'failed', message: result.message } });
  }
}

export function BootGate({ children }: { children: ReactNode }) {
  const state = useBoot();
  useEffect(() => {
    void boot();
  }, []);
  useEffect(() => {
    if (state.state !== 'starting') void SplashScreen.hideAsync().catch(() => {});
  }, [state.state]);
  const retry = useCallback(() => void boot(), []);
  if (state.state === 'starting') return <BootScreen />;
  if (state.state === 'failed') return <BootScreen failure={state.message} onRetry={retry} />;
  return <>{children}</>;
}
