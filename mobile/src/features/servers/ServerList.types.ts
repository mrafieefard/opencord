import type { ServerRow } from '@/state/types';

export interface ServerListProps {
  rows: ServerRow[];
  /** The list's clock, so times move on with it. */
  nowMs: number;
  locale: string;
  onOpen: (serverKey: string) => void;
  refreshing: boolean;
  onRefresh: () => Promise<void>;
  /** Shown instead of rows when a search finds none. */
  emptyTitle?: string;
}
