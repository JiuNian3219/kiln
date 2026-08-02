import { open } from '@tauri-apps/plugin-dialog';

// Single browser-side boundary for Tauri APIs. Components receive actions and
// data; they do not reach into window.__TAURI__ directly.
export const tauri = window.__TAURI__;

export const invoke = (command, args = {}) => tauri.core.invoke(command, args);

export const saveReference = () => invoke('save_reference');

export const clearReference = () => invoke('clear_reference');

export const listen = (event, handler) => tauri.event.listen(event, handler);

export const reportClientDiagnostic = (kind, errorType) =>
  invoke('report_client_diagnostic', { kind, errorType });

export const chooseDirectory = (title) => open({ title, directory: true, multiple: false });
export const chooseDirectories = (title) => open({ title, directory: true, multiple: true });
export const chooseMarkdownFile = (title) =>
  open({ title, multiple: false, filters: [{ name: 'Markdown', extensions: ['md'] }] });
