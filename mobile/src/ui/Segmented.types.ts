export interface SegmentedProps {
  values: string[];
  index: number;
  onChange: (index: number) => void;
}
