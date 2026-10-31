// Android draws a sheet in the app's own window, so toasts drawn at the
// root stay over it. (ToastHost.ios.tsx lifts them over iOS's sheets.)
export { Toasts as ToastHost } from './Toasts';
