import { useCallback, useEffect, useRef, useState } from 'react';
import type { DownloadEvent, Update } from '@tauri-apps/plugin-updater';
import {
  checkForAppUpdate,
  closeAppUpdate,
  getCurrentAppVersion,
  installAppUpdate,
  updateErrorMessage,
  type AppUpdateInfo,
} from '../lib/updater';

export type AppDistribution = 'installed' | 'portable';
export type UpdatePhase = 'idle' | 'checking' | 'up-to-date' | 'available' | 'installing' | 'error';

export interface AppUpdaterState {
  supported: boolean;
  phase: UpdatePhase;
  currentVersion: string | null;
  info: AppUpdateInfo | null;
  error: string | null;
  progress: number | null;
  checkForUpdates: () => Promise<void>;
  installUpdate: () => Promise<void>;
}

export function useAppUpdater(distribution: AppDistribution | undefined, supported: boolean): AppUpdaterState {
  const [phase, setPhase] = useState<UpdatePhase>('idle');
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);
  const [info, setInfo] = useState<AppUpdateInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [update, setUpdate] = useState<Update | null>(null);
  const updateRef = useRef<Update | null>(null);
  const initialCheckStarted = useRef(false);
  const busyRef = useRef(false);

  const replaceUpdate = useCallback(async (next: Update | null) => {
    const previous = updateRef.current;
    updateRef.current = next;
    setUpdate(next);
    if (previous && previous !== next) {
      await closeAppUpdate(previous);
    }
  }, []);

  const checkForUpdates = useCallback(async () => {
    if (!supported || busyRef.current) return;
    busyRef.current = true;

    setPhase('checking');
    setError(null);
    setProgress(null);

    try {
      const result = await checkForAppUpdate();
      await replaceUpdate(result.update);
      setCurrentVersion(result.info.currentVersion);
      setInfo(result.info);
      setPhase(result.update ? 'available' : 'up-to-date');
    } catch (reason) {
      setPhase('error');
      setError(updateErrorMessage(reason));
    } finally {
      busyRef.current = false;
    }
  }, [replaceUpdate, supported]);

  const installUpdateNow = useCallback(async () => {
    if (!supported || !update || distribution === 'portable' || busyRef.current) return;
    busyRef.current = true;

    setPhase('installing');
    setError(null);
    setProgress(null);
    let contentLength: number | undefined;
    let downloadedBytes = 0;

    const onEvent = (event: DownloadEvent) => {
      if (event.event === 'Started') {
        contentLength = event.data.contentLength;
        downloadedBytes = 0;
        setProgress(contentLength ? 0 : null);
      } else if (event.event === 'Progress') {
        downloadedBytes += event.data.chunkLength;
        setProgress(contentLength ? Math.min(downloadedBytes / contentLength, 1) : null);
      } else {
        setProgress(1);
      }
    };

    try {
      await installAppUpdate(update, onEvent);
    } catch (reason) {
      setPhase('error');
      setError(updateErrorMessage(reason));
    } finally {
      busyRef.current = false;
    }
  }, [distribution, supported, update]);

  useEffect(() => {
    void getCurrentAppVersion().then(setCurrentVersion).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!supported || !distribution || initialCheckStarted.current) return;
    initialCheckStarted.current = true;
    const timer = window.setTimeout(() => {
      void checkForUpdates().catch(() => undefined);
    }, 1_500);
    return () => window.clearTimeout(timer);
  }, [checkForUpdates, distribution, supported]);

  useEffect(() => () => {
    const current = updateRef.current;
    if (current) void closeAppUpdate(current);
  }, []);

  return {
    supported,
    phase,
    currentVersion,
    info,
    error,
    progress,
    checkForUpdates,
    installUpdate: installUpdateNow,
  };
}
