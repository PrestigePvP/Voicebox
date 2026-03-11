import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Mode = "cloud" | "local";

interface Config {
  provider: { mode: Mode };
  cloud: {
    account_id: string;
    api_token: string;
    stt_model: string;
    formatter_model: string;
    worker_url: string;
    token: string;
  };
  local: {
    server_url: string;
    token: string;
  };
  audio: {
    sample_rate: number;
    channels: number;
    chunk_size: number;
  };
  hotkey: { record: string };
  beta: { streaming_stt: boolean };
  overlay_position: string;
}

export type { Config, Mode };

export const useConfig = () => {
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [configPath, setConfigPath] = useState("");

  useEffect(() => {
    Promise.all([
      invoke<Config>("get_config"),
      invoke<string>("get_config_path"),
    ]).then(([cfg, path]) => {
      setConfig(cfg);
      setConfigPath(path);
      setLoading(false);
    });
  }, []);

  const save = useCallback(async (cfg: Config) => {
    await invoke("save_config", { cfg });
    setConfig(cfg);
  }, []);

  return { config, loading, configPath, save };
};
