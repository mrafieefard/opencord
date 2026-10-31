// Turns an external invite link into the internal route for the Add server sheet.
export function inviteRoute(url: string): string | null {
  if (!url) return null;
  if (/^opencords?:\/\//i.test(url)) {
    return `/add-server?link=${encodeURIComponent(url)}`;
  }
  return null;
}
