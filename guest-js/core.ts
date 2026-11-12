// The plugin's commands, and what they reject with.

import { invoke } from '@tauri-apps/api/core';

/** A refusal: `code` is `declined`, `busy`, `consent_required`, `bad_token`, `library_missing`, `secret_key_in_app`, `permission_required`, … */
export class SupportError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = 'SupportError';
  }
}

export const PLUGIN = 'plugin:gaiadesk-support|';

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(PLUGIN + cmd, args);
  } catch (e) {
    const o = (e ?? {}) as { code?: unknown; message?: unknown };
    throw new SupportError(typeof o.code === 'string' ? o.code : 'internal', typeof o.message === 'string' ? o.message : String(e));
  }
}
