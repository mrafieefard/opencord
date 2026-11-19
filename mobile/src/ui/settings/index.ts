// The settings kit: every settings page is a SettingsList of SettingsSections
// of SettingsRows, with useSaveButton when it edits a draft.
export { SettingsList } from './SettingsList';
export { SettingsRow, type SettingsRowProps } from './SettingsRow';
export type { ChoiceOption } from './SettingsRow.types';
export { SettingsSection, type SettingsSectionProps } from './SettingsSection';
export { useSaveButton, type SaveButtonOptions } from './useSaveButton';
