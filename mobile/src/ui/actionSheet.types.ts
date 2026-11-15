export interface ActionSheetAction {
  id: string;
  title: string;
  destructive?: boolean;
}

export interface ActionSheetOptions {
  title?: string;
  actions: ActionSheetAction[];
  onAction: (id: string) => void;
  onCancel?: () => void;
}
