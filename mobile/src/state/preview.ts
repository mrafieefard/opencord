// A message as one line of plain text for the server list: the desktop's
// markdown subset stripped, mentions and channel links by name.
export interface PreviewLookup {
  member(id: string): string | undefined;
  channel(id: string): string | undefined;
}

export function previewText(content: string, lookup: PreviewLookup): string {
  return content
    .replace(/```[^\n]*\n?([\s\S]*?)```/g, '$1')
    .replace(/<@(\d+)>/g, (_, id: string) => `@${lookup.member(id) ?? 'unknown'}`)
    .replace(/<#(\d+)>/g, (_, id: string) => `#${lookup.channel(id) ?? 'unknown'}`)
    .replace(/^>\s?/gm, '')
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/~~(.+?)~~/g, '$1')
    .replace(/(^|[^\w*])\*(?!\s)(.+?)\*(?!\w)/g, '$1$2')
    .replace(/(^|[^\w_])_(?!\s)(.+?)_(?!\w)/g, '$1$2')
    .replace(/`([^`\n]*)`/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}
