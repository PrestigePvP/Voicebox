import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ModelInfo {
  id: string;
  name: string;
  note: string;
  size_bytes: number;
  downloaded: boolean;
}

type ModelStatus = "ready" | "loading" | "failed" | "missing";

interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number;
}

export type { ModelInfo, ModelStatus, DownloadProgress };

export const formatBytes = (bytes: number): string => {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  return `${Math.round(bytes / 1_000_000)} MB`;
};

export const useModels = () => {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [status, setStatus] = useState<ModelStatus>("missing");
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [list, modelStatus] = await Promise.all([
      invoke<ModelInfo[]>("list_models"),
      invoke<ModelStatus>("get_model_status"),
    ]);
    setModels(list);
    setStatus(modelStatus);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const unlisten = [
      listen<DownloadProgress>("voicebox:model_download_progress", (e) => {
        setProgress((prev) => ({ ...prev, [e.payload.id]: e.payload }));
      }),
      listen<string>("voicebox:model_download_complete", (e) => {
        setProgress((prev) => {
          const next = { ...prev };
          delete next[e.payload];
          return next;
        });
        refresh();
      }),
      listen<{ id: string; message: string }>("voicebox:model_download_error", (e) => {
        setProgress((prev) => {
          const next = { ...prev };
          delete next[e.payload.id];
          return next;
        });
        setError(e.payload.message);
        refresh();
      }),
      listen<ModelStatus>("voicebox:model_status", (e) => {
        setStatus(e.payload);
      }),
    ];

    return () => {
      unlisten.forEach((p) => p.then((fn) => fn()));
    };
  }, [refresh]);

  const download = useCallback(
    async (id: string) => {
      setError(null);
      setProgress((prev) => ({ ...prev, [id]: { id, downloaded: 0, total: 0 } }));
      try {
        await invoke("download_model", { id });
      } catch (e) {
        setError(String(e));
        setProgress((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
      }
    },
    [],
  );

  const remove = useCallback(
    async (id: string) => {
      setError(null);
      try {
        await invoke("delete_model", { id });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  return { models, status, progress, error, download, remove, refresh };
};
