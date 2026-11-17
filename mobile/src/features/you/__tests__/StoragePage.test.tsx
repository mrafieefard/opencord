import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react-native';

import { FakeCore } from '@/core/fake/FakeCore';
import { StoragePage } from '../StoragePage';
import { confirm } from '@/ui';
import * as Sharing from 'expo-sharing';

jest.mock('@/ui', () => {
  const actual = jest.requireActual('@/ui');
  return {
    ...actual,
    confirm: jest.fn(),
  };
});

jest.mock('expo-sharing', () => ({
  shareAsync: jest.fn().mockResolvedValue(true),
}));

const mockPush = jest.fn();
jest.mock('expo-router', () => ({
  useRouter: () => ({ push: mockPush }),
  useNavigation: () => ({ setOptions: jest.fn(), addListener: jest.fn(() => () => {}) }),
}));

describe('StoragePage', () => {
  let core: FakeCore;

  beforeEach(() => {
    jest.clearAllMocks();
    core = new FakeCore();
  });

  afterEach(() => {
    cleanup();
  });

  // Fails if clear does not confirm via dialog or if cacheClear is not invoked or if the size is not updated to 0 B.
  it('clear asks, clears and shows the new size', async () => {
    (confirm as jest.Mock).mockImplementation((opts) => opts.onConfirm());

    await act(async () => {
      render(<StoragePage core={core} />);
    });

    // Initially shows calculated size 42.0 MB
    await waitFor(() => {
      expect(screen.getByText('42.0 MB')).toBeTruthy();
    });

    const clearButton = screen.getByText('Clear media cache');
    await act(async () => {
      fireEvent.press(clearButton);
    });

    expect(confirm).toHaveBeenCalledWith(
      expect.objectContaining({
        title: 'Clear media cache?',
        confirmTitle: 'Clear',
        destructive: true,
      }),
    );

    await waitFor(() => {
      expect(core.calls.some((c) => c.method === 'cacheClear')).toBe(true);
      expect(screen.getByText('0 B')).toBeTruthy();
    });
  });

  // Fails if export row does not call logsExport or does not invoke Sharing.shareAsync with the exported path.
  it('export shares the session log', async () => {
    await act(async () => {
      render(<StoragePage core={core} />);
    });

    await waitFor(() => {
      expect(screen.getByText("Export this session's log")).toBeTruthy();
    });

    const exportButton = screen.getByText("Export this session's log");
    await act(async () => {
      fireEvent.press(exportButton);
    });

    await waitFor(() => {
      expect(core.calls.some((c) => c.method === 'logsExport')).toBe(true);
      expect(Sharing.shareAsync).toHaveBeenCalledWith(
        '/fake/logs/opencord-current.log',
      );
    });
  });

  // Fails if crashed log row is shown when no crash occurred or omitted when a crashed session exists.
  it('shows and exports crashed session log when previous session crashed', async () => {
    core.logsCrashed = jest.fn().mockResolvedValue('/fake/logs/crashed.log');

    await act(async () => {
      render(<StoragePage core={core} />);
    });

    await waitFor(() => {
      expect(screen.getByText("Export the last session's log")).toBeTruthy();
    });

    const exportCrashedButton = screen.getByText("Export the last session's log");
    await act(async () => {
      fireEvent.press(exportCrashedButton);
    });

    await waitFor(() => {
      expect(Sharing.shareAsync).toHaveBeenCalledWith('/fake/logs/crashed.log');
    });
  });
});
