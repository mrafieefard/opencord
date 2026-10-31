import { fireEvent, render, waitFor } from '@testing-library/react-native';
import { AttachmentKind, AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';
import * as Sharing from 'expo-sharing';

import { setCore } from '@/core';
import { FakeCore } from '@/core/fake/FakeCore';

import { ImageAttachment } from '../ImageAttachment';
import { FileAttachment } from '../FileAttachment';

jest.mock('expo-router', () => ({
  router: { push: jest.fn(), back: jest.fn() },
}));

describe('Attachments', () => {
  const serverKey = 'test-server';

  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('an image shows a placeholder of its size, then its thumbnail', async () => {
    let resolveAttachment: (path: string) => void = () => {};
    const attachmentPromise = new Promise<string>((resolve) => {
      resolveAttachment = resolve;
    });

    const fakeCore = new FakeCore({ scenario: 'empty' });
    jest.spyOn(fakeCore, 'attachmentFile').mockImplementation(async (_sk, id, variant) => {
      if (variant === AttachmentVariant.Thumbnail) {
        return attachmentPromise;
      }
      return `/fake/attachments/${id}.o`;
    });
    setCore(fakeCore);

    const attachment: AttachmentRecord = {
      id: 'img-1',
      filename: 'photo.jpg',
      contentType: 'image/jpeg',
      sizeBytes: 1024,
      kind: AttachmentKind.Image,
      width: 800,
      height: 600,
      hasThumbnail: true,
      animated: false,
      waveform: [],
    };

    const { getByTestId, queryByTestId } = await render(
      <ImageAttachment maxWidth={300} serverKey={serverKey} attachment={attachment} />,
    );

    // fitSize(800, 600) -> 260 × 195
    // While loading, placeholder is shown with 260 × 195
    expect(getByTestId('attachment-placeholder-img-1')).toBeTruthy();
    expect(queryByTestId('attachment-image-img-1')).toBeNull();

    // Now thumbnail resolves
    resolveAttachment('/fake/attachments/img-1.t');

    await waitFor(() => {
      expect(getByTestId('attachment-image-img-1')).toBeTruthy();
    });
    expect(queryByTestId('attachment-placeholder-img-1')).toBeNull();
  });

  it('a file card shows the name, size and kind', async () => {
    const fakeCore = new FakeCore({ scenario: 'empty' });
    setCore(fakeCore);

    const attachment: AttachmentRecord = {
      id: 'file-1',
      filename: 'document.pdf',
      contentType: 'application/pdf',
      sizeBytes: 1048576, // 1.0 MB
      kind: AttachmentKind.File,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    const { getByText } = await render(
      <FileAttachment maxWidth={300} serverKey={serverKey} attachment={attachment} />,
    );

    expect(getByText('document.pdf')).toBeTruthy();
    expect(getByText(/1\.0 MB/)).toBeTruthy();
    expect(getByText(/File/)).toBeTruthy();
  });

  it('tapping a file downloads it once and opens sharing, even with two quick taps', async () => {
    const fakeCore = new FakeCore({ scenario: 'empty' });
    let calls = 0;
    jest.spyOn(fakeCore, 'attachmentFile').mockImplementation(async () => {
      calls++;
      await new Promise((r) => setTimeout(r, 10));
      return '/fake/attachments/file-1.o';
    });
    setCore(fakeCore);

    const attachment: AttachmentRecord = {
      id: 'file-1',
      filename: 'document.pdf',
      contentType: 'application/pdf',
      sizeBytes: 1048576,
      kind: AttachmentKind.File,
      hasThumbnail: false,
      animated: false,
      waveform: [],
    };

    const { getByTestId } = await render(
      <FileAttachment maxWidth={300} serverKey={serverKey} attachment={attachment} />,
    );

    const card = getByTestId('file-attachment-file-1');

    // Two quick taps
    fireEvent.press(card);
    fireEvent.press(card);

    await waitFor(() => {
      expect(Sharing.shareAsync).toHaveBeenCalledTimes(1);
    });

    expect(calls).toBe(1);
  });
});
