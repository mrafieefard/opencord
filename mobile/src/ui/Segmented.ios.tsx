// SwiftUI's segmented picker (design §10).
import { Host, Picker, Text } from '@expo/ui/swift-ui';
import { pickerStyle, tag } from '@expo/ui/swift-ui/modifiers';

import type { SegmentedProps } from './Segmented.types';
import { useTheme } from './theme/useTheme';

export type { SegmentedProps } from './Segmented.types';

export function Segmented({ values, index, onChange }: SegmentedProps) {
  const { scheme } = useTheme();
  return (
    <Host matchContents={{ vertical: true }} colorScheme={scheme} style={{ alignSelf: 'stretch' }}>
      <Picker<number> selection={index} onSelectionChange={onChange} modifiers={[pickerStyle('segmented')]}>
        {values.map((value, position) => (
          <Text key={value} modifiers={[tag(position)]}>
            {value}
          </Text>
        ))}
      </Picker>
    </Host>
  );
}
