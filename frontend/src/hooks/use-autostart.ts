import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AutostartStatus {
  enabled: boolean;
  installed: boolean;
}

export const useAutostart = () => {
  const [status, setStatus] = useState<AutostartStatus>({
    enabled: false,
    installed: false,
  });

  useEffect(() => {
    invoke<AutostartStatus>("get_autostart").then(setStatus);
  }, []);

  const setEnabled = useCallback(async (enabled: boolean) => {
    await invoke("set_autostart", { enabled });
    setStatus((prev) => ({ ...prev, enabled }));
  }, []);

  return { ...status, setEnabled };
};
