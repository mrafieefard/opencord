export interface ConfirmOptions {
  title: string;
  message?: string;
  /** The confirming button; the other is always Cancel. */
  confirmTitle: string;
  destructive?: boolean;
  onConfirm: () => void;
  /** Cancel, or the dialog dismissed without an answer. */
  onCancel?: () => void;
}
