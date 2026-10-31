// Intercepts deep links from the operating system to route invite links to the Add server sheet.
import { inviteRoute } from '@/features/links/inviteLink';

export function redirectSystemPath({
  path,
}: {
  path: string;
  initial: boolean;
}): string {
  return inviteRoute(path) ?? path;
}
