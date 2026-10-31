import { UploadPhaseRecord } from '@opencord/core-native';

import { uploadText } from '../uploadText';

describe('uploadText', () => {
  it('each phase reads as the spec says', () => {
    expect(uploadText(new UploadPhaseRecord.Preparing(), 0, 1000)).toBe('Checking…');
    expect(uploadText(new UploadPhaseRecord.Processing(), 1000, 1000)).toBe('The server is checking it…');
    expect(uploadText(new UploadPhaseRecord.Done(), 1000, 1000)).toBe('Queued');
    expect(
      uploadText(
        new UploadPhaseRecord.Failed({ message: "That file isn't a song this server can play." }),
        1000,
        1000,
      ),
    ).toBe("Failed: That file isn't a song this server can play.");
    expect(uploadText(new UploadPhaseRecord.Cancelled(), 500, 1000)).toBe('Cancelled');
  });

  it('formats upload progress including sizes in KB, MB and GB', () => {
    // 42% of 8.4 MB
    const totalMb = 8.4 * 1024 * 1024;
    const sentMb = Math.round(totalMb * 0.42);
    expect(uploadText(new UploadPhaseRecord.Uploading(), sentMb, totalMb, 'en-US')).toBe(
      'Uploading · 42 % of 8.4 MB',
    );

    // 50% of 500 KB
    const totalKb = 500 * 1024;
    const sentKb = 250 * 1024;
    expect(uploadText(new UploadPhaseRecord.Uploading(), sentKb, totalKb, 'en-US')).toBe(
      'Uploading · 50 % of 500 KB',
    );

    // 10% of 1.5 GB
    const totalGb = 1.5 * 1024 * 1024 * 1024;
    const sentGb = Math.round(totalGb * 0.1);
    expect(uploadText(new UploadPhaseRecord.Uploading(), sentGb, totalGb, 'en-US')).toBe(
      'Uploading · 10 % of 1.5 GB',
    );
  });
});
