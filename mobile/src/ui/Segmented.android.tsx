// Material's single-choice segmented buttons (design §10), monochrome.
import { Host, SegmentedButton, SingleChoiceSegmentedButtonRow, Text } from '@expo/ui/jetpack-compose';

import type { SegmentedProps } from './Segmented.types';
import { useTheme } from './theme/useTheme';

export type { SegmentedProps } from './Segmented.types';

export function Segmented({ values, index, onChange }: SegmentedProps) {
  const { colors, scheme } = useTheme();
  return (
    <Host matchContents={{ vertical: true }} colorScheme={scheme} style={{ alignSelf: 'stretch' }}>
      <SingleChoiceSegmentedButtonRow>
        {values.map((value, position) => (
          <SegmentedButton
            key={value}
            selected={position === index}
            onClick={() => onChange(position)}
            colors={{
              activeContainerColor: colors.selected,
              activeContentColor: colors.text,
              activeBorderColor: colors.border,
              inactiveContainerColor: colors.surface,
              inactiveContentColor: colors.textSecondary,
              inactiveBorderColor: colors.border,
            }}
          >
            <SegmentedButton.Label>
              <Text>{value}</Text>
            </SegmentedButton.Label>
          </SegmentedButton>
        ))}
      </SingleChoiceSegmentedButtonRow>
    </Host>
  );
}
