// `/`: onboarding before an identity, the servers after.
import { Redirect } from 'expo-router';

import { useBoot } from '@/state/selectors';

export default function Index() {
  const boot = useBoot();
  return <Redirect href={boot.state === 'ready' ? '/servers' : '/welcome'} />;
}
