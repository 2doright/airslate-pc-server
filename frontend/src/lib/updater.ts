import { getVersion } from '@tauri-apps/api/app';
import {
  check,
  type DownloadEvent,
  type Update,
} from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export interface AppUpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes: string;
  pubDate: string | null;
}

export async function getCurrentAppVersion(): Promise<string> {
  return getVersion();
}

export async function checkForAppUpdate(): Promise<{ update: Update | null; info: AppUpdateInfo }> {
  const currentVersion = await getCurrentAppVersion();
  const update = await check({ timeout: 30_000 });

  return {
    update,
    info: {
      currentVersion: update?.currentVersion ?? currentVersion,
      availableVersion: update?.version ?? currentVersion,
      notes: update?.body ?? '',
      pubDate: update?.date ?? null,
    },
  };
}

export async function installAppUpdate(
  update: Update,
  onEvent: (event: DownloadEvent) => void,
): Promise<void> {
  await update.downloadAndInstall(onEvent);
  await relaunch();
}

export async function closeAppUpdate(update: Update | null): Promise<void> {
  if (update) {
    await update.close();
  }
}

export function updateErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
