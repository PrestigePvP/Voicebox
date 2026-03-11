import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { zodResolver } from "@hookform/resolvers/zod";
import { useConfig, type Mode } from "../hooks/use-config";
import { cn } from "../lib/utils";

const overlayPositions = [
  { value: "top_center", label: "Top Center" },
  { value: "bottom_left", label: "Bottom Left" },
  { value: "bottom_center", label: "Bottom Center" },
  { value: "bottom_right", label: "Bottom Right" },
] as const;

const cloudSchema = z.object({
  mode: z.literal("cloud"),
  hotkey: z.string().min(1, "Hotkey is required"),
  overlayPosition: z.string(),
  workerUrl: z.string().min(1, "Worker URL is required"),
  cloudToken: z.string().min(1, "Auth token is required"),
  serverUrl: z.string(),
  localToken: z.string(),
  streamingStt: z.boolean(),
});

const localSchema = z.object({
  mode: z.literal("local"),
  hotkey: z.string().min(1, "Hotkey is required"),
  overlayPosition: z.string(),
  workerUrl: z.string(),
  cloudToken: z.string(),
  serverUrl: z.string().min(1, "Server URL is required"),
  localToken: z.string(),
  streamingStt: z.boolean(),
});

const schema = z.discriminatedUnion("mode", [cloudSchema, localSchema]);

type FormValues = z.infer<typeof schema>;

const SettingsForm = () => {
  const { config, loading, configPath, save } = useConfig();
  const [saveStatus, setSaveStatus] = useState<"idle" | "saved" | "error">("idle");

  const {
    register,
    handleSubmit,
    watch,
    reset,
    formState: { errors, isDirty },
  } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      mode: "cloud",
      hotkey: "",
      overlayPosition: "top_center",
      workerUrl: "",
      cloudToken: "",
      serverUrl: "",
      localToken: "",
      streamingStt: false,
    },
  });

  useEffect(() => {
    if (config) {
      reset({
        mode: config.provider.mode,
        hotkey: config.hotkey.record,
        overlayPosition: config.overlay_position ?? "top_center",
        workerUrl: config.cloud.worker_url,
        cloudToken: config.cloud.token,
        serverUrl: config.local.server_url,
        localToken: config.local.token,
        streamingStt: config.beta?.streaming_stt ?? false,
      });
    }
  }, [config, reset]);

  const mode = watch("mode");

  const onSubmit = async (values: FormValues) => {
    if (!config) return;
    setSaveStatus("idle");

    const updated = {
      ...config,
      provider: { mode: values.mode as Mode },
      hotkey: { record: values.hotkey },
      overlay_position: values.overlayPosition,
      cloud: {
        ...config.cloud,
        worker_url: values.workerUrl,
        token: values.cloudToken,
      },
      local: {
        ...config.local,
        server_url: values.serverUrl,
        token: values.localToken,
      },
      beta: {
        streaming_stt: values.streamingStt,
      },
    };

    try {
      await save(updated);
      setSaveStatus("saved");
      setTimeout(() => setSaveStatus("idle"), 2000);
    } catch {
      setSaveStatus("error");
      setTimeout(() => setSaveStatus("idle"), 3000);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-zinc-500">
        Loading...
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-6 p-6">
      <div className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold text-zinc-100">General</h2>

        <Field label="Hotkey" error={errors.hotkey?.message}>
          <input
            {...register("hotkey")}
            placeholder="ctrl+cmd+r"
            className="input"
          />
        </Field>

        <Field label="Overlay Position">
          <select {...register("overlayPosition")} className="input">
            {overlayPositions.map((p) => (
              <option key={p.value} value={p.value}>{p.label}</option>
            ))}
          </select>
        </Field>

        <Field label="Provider">
          <div className="flex gap-2">
            <label className={cn("radio-card", mode === "cloud" && "radio-card-active")}>
              <input type="radio" value="cloud" {...register("mode")} className="sr-only" />
              <span>Cloud</span>
            </label>
            <label className={cn("radio-card", mode === "local" && "radio-card-active")}>
              <input type="radio" value="local" {...register("mode")} className="sr-only" />
              <span>Local</span>
            </label>
          </div>
        </Field>
      </div>

      {mode === "cloud" && (
        <div className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold text-zinc-100">Cloud</h2>

          <Field label="Worker URL" error={errors.workerUrl?.message}>
            <input
              {...register("workerUrl")}
              placeholder="https://voicebox.example.workers.dev"
              className="input"
            />
          </Field>

          <Field label="Auth Token" error={errors.cloudToken?.message}>
            <input
              {...register("cloudToken")}
              type="password"
              placeholder="••••••••"
              className="input"
            />
          </Field>
        </div>
      )}

      {mode === "local" && (
        <div className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold text-zinc-100">Local</h2>

          <Field label="Server URL" error={errors.serverUrl?.message}>
            <input
              {...register("serverUrl")}
              placeholder="http://192.168.1.183:9090"
              className="input"
            />
          </Field>

          <Field label="Auth Token (optional)">
            <input
              {...register("localToken")}
              type="password"
              placeholder="••••••••"
              className="input"
            />
          </Field>
        </div>
      )}

      <div className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold text-zinc-100">Beta</h2>
        <label className="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            {...register("streamingStt")}
            className="h-4 w-4 rounded border-zinc-600 bg-zinc-800 text-blue-600 focus:ring-blue-500 focus:ring-offset-zinc-900"
          />
          <span className="text-sm text-zinc-400">Live transcription preview</span>
        </label>
      </div>

      <div className="flex items-center gap-3 mt-auto pt-4 border-t border-zinc-800">
        <button
          type="submit"
          disabled={!isDirty}
          className="px-4 py-2 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
        >
          Save
        </button>
        {saveStatus === "saved" && (
          <span className="text-sm text-green-400">Settings saved</span>
        )}
        {saveStatus === "error" && (
          <span className="text-sm text-red-400">Failed to save</span>
        )}
        <span className="ml-auto text-xs text-zinc-600 truncate max-w-48" title={configPath}>
          {configPath}
        </span>
      </div>
    </form>
  );
};

const Field = ({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: React.ReactNode;
}) => (
  <div className="flex flex-col gap-1.5">
    <label className="text-sm font-medium text-zinc-400">{label}</label>
    {children}
    {error && <p className="text-xs text-red-400">{error}</p>}
  </div>
);

export default SettingsForm;
