import { Platform } from 'react-native';

import * as android from './actionSheet.android';
import * as ios from './actionSheet.ios';

export type { ActionSheetAction, ActionSheetOptions } from './actionSheet.types';

export const showActionSheet = Platform.OS === 'android' ? android.showActionSheet : ios.showActionSheet;
