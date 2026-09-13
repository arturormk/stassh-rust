import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

declare global {
  interface Window {
    __STASSH_TEST_API__?: {
      invoke: typeof tauriInvoke;
      listen: typeof tauriListen;
      emit?: (eventName: string, payload: unknown) => void;
      resizeCalls?: { sessionId: string; cols: number; rows: number }[];
    };
  }
}

export const invoke = window.__STASSH_TEST_API__?.invoke ?? tauriInvoke;
export const listen = window.__STASSH_TEST_API__?.listen ?? tauriListen;
