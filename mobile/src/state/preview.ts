import { findFences } from '../features/chat/markdown';

// A message as one line of plain text for the server list: the desktop's
// markdown subset stripped, mentions and channel links by name.
export interface PreviewLookup {
  member(id: string): string | undefined;
  channel(id: string): string | undefined;
}

export function previewText(content: string, lookup: PreviewLookup): string {
  const fences = findFences(content);
  let text = '';
  let cursor = 0;
  for (const fence of fences) {
    text += content.substring(cursor, fence.start);
    text += ` ${fence.code} `;
    cursor = fence.end;
  }
  text += content.substring(cursor);

  return text
    .replace(/<@(\d+)>/g, (_, id: string) => `@${lookup.member(id) ?? 'unknown'}`)
    .replace(/<#(\d+)>/g, (_, id: string) => `#${lookup.channel(id) ?? 'unknown'}`)
    .replace(/^>\s?/gm, '')
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/~~(.+?)~~/g, '$1')
    .replace(/(^|[^\w*])\*(?!\s)(.+?)\*(?!\w)/g, '$1$2')
    .replace(/(^|[^\w_])_(?!\s)(.+?)_(?!\w)/g, '$1$2')
    .replace(/`([^`\n]+)`/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}
