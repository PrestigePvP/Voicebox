import { useCallback, useEffect, useState } from "react";

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
    stt_endpoint: string;
    formatter_endpoint: string;
    formatter_model: string;
  };
  audio: {
    sample_rate: number;
    channels: number;
    chunk_size: number;
  };
  hotkey: { record: string };
}

declare global {
  interface Window {
    go: {
      main: {
        App: {
          GetConfig: () => Promise<Config>;
          SaveConfig: (cfg: Config) => Promise<void>;
          GetConfigPath: () => Promise<string>;
        };
      };
    };
  }
}

export type { Config, Mode };

export const useConfig = () => {
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [configPath, setConfigPath] = useState("");

  useEffect(() => {
    Promise.all([
      window.go.main.App.GetConfig(),
      window.go.main.App.GetConfigPath(),
    ]).then(([cfg, path]) => {
      setConfig(cfg);
      setConfigPath(path);
      setLoading(false);
    });
  }, []);

  const save = useCallback(async (cfg: Config) => {
    await window.go.main.App.SaveConfig(cfg);
    setConfig(cfg);
  }, []);

  return { config, loading, configPath, save };
};
