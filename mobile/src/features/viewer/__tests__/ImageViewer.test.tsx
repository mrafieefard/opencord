import { render } from '@testing-library/react-native';
import { GestureHandlerRootView, State } from 'react-native-gesture-handler';
import { fireGestureHandler, getByGestureTestId } from 'react-native-gesture-handler/jest-utils';

import { FakeCore } from '@/core/fake/FakeCore';
import { setCore } from '@/core';
import { ImageViewer } from '../ImageViewer';

const mockBack = jest.fn();
jest.mock('expo-router', () => ({
  router: {
    back: () => mockBack(),
    push: jest.fn(),
  },
}));

describe('ImageViewer', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    setCore(new FakeCore({ scenario: 'empty' }));
  });

  it('the viewer closes on a swipe down', async () => {
    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ImageViewer serverKey="fake-server" attachmentId="img-1" />
      </GestureHandlerRootView>,
    );

    fireGestureHandler(getByGestureTestId('viewer-pan'), [
      { state: State.BEGAN, translationY: 0 },
      { state: State.ACTIVE, translationY: 150 },
      { state: State.END, translationY: 150 },
    ]);

    expect(mockBack).toHaveBeenCalledTimes(1);
  });
});
