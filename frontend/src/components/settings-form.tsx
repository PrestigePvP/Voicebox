import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { zodResolver } from "@hookform/resolvers/zod";
import { useConfig, type Mode } from "../hooks/use-config";
import { formatBytes, useModels, type ModelStatus } from "../hooks/use-models";
import { cn } from "../lib/utils";

const overlayPositions = [
  { value: "top_center", label: "Top Center" },
  { value: "bottom_left", label: "Bottom Left" },
  { value: "bottom_center", label: "Bottom Center" },
  { value: "bottom_right", label: "Bottom Right" },
] as const;

const formatters = [
  { value: "none", label: "None — use Whisper output directly" },
  { value: "ollama", label: "Ollama — app-aware formatting" },
] as const;

const cloudSchema = z.object({
  mode: z.literal("cloud"),
  hotkey: z.string().min(1, "Hotkey is required"),
  overlayPosition: z.string(),
  workerUrl: z.string().min(1, "Worker URL is required"),
  cloudToken: z.string().min(1, "Auth token is required"),
  model: z.string(),
  formatter: z.string(),
  ollamaUrl: z.string(),
  ollamaModel: z.string(),
  streamingStt: z.boolean(),
});

const localSchema = z.object({
  mode: z.literal("local"),
  hotkey: z.string().min(1, "Hotkey is required"),
  overlayPosition: z.string(),
  workerUrl: z.string(),
  cloudToken: z.string(),
  model: z.string().min(1, "Pick a model"),
  formatter: z.string(),
  ollamaUrl: z.string(),
  ollamaModel: z.string(),
  streamingStt: z.boolean(),
});

const schema = z.discriminatedUnion("mode", [cloudSchema, localSchema]);

type FormValues = z.infer<typeof schema>;

const SettingsForm = () => {
  const { config, loading, configPath, save } = useConfig();
  const { models, status, progress, error: modelError, download, remove } = useModels();
  const [saveStatus, setSaveStatus] = useState<"idle" | "saved" | "error">("idle");

  const {
    register,
    handleSubmit,
    watch,
    setValue,
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
      model: "small.en",
      formatter: "none",
      ollamaUrl: "http://localhost:11434",
      ollamaModel: "qwen3:4b",
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
        model: config.local.model,
        formatter: config.local.formatter,
        ollamaUrl: config.local.ollama_url,
        ollamaModel: config.local.ollama_model,
        streamingStt: config.beta?.streaming_stt ?? false,
      });
    }
  }, [config, reset]);

  const mode = watch("mode");
  const selectedModel = watch("model");
  const formatter = watch("formatter");

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
        model: values.model,
        formatter: values.formatter,
        ollama_url: values.ollamaUrl,
        ollama_model: values.ollamaModel,
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
              <span>Local (on-device)</span>
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
          <div className="flex items-baseline gap-3">
            <h2 className="text-lg font-semibold text-zinc-100">Speech Model</h2>
            <ModelStatusBadge status={status} />
          </div>
          <p className="text-xs text-zinc-500 -mt-2">
            Runs entirely on this Mac. Nothing is sent over the network.
          </p>

          {errors.model?.message && (
            <p className="text-xs text-red-400">{errors.model.message}</p>
          )}
          {modelError && <p className="text-xs text-red-400">{modelError}</p>}

          <div className="flex flex-col gap-2">
            {models.map((m) => {
              const active = selectedModel === m.id;
              const downloading = progress[m.id];
              const pct = downloading?.total
                ? Math.round((downloading.downloaded / downloading.total) * 100)
                : 0;

              return (
                <div
                  key={m.id}
                  className={cn(
                    "rounded-lg border px-3 py-2.5 transition-colors",
                    active
                      ? "border-blue-500/60 bg-blue-500/10"
                      : "border-zinc-800 bg-zinc-900/40",
                  )}
                >
                  <div className="flex items-center gap-3">
                    <button
                      type="button"
                      disabled={!m.downloaded}
                      onClick={() =>
                        setValue("model", m.id, { shouldDirty: true })
                      }
                      className="flex-1 text-left disabled:cursor-not-allowed"
                    >
                      <div className="flex items-center gap-2">
                        <span
                          className={cn(
                            "text-sm font-medium",
                            m.downloaded ? "text-zinc-100" : "text-zinc-500",
                          )}
                        >
                          {m.name}
                        </span>
                        <span className="text-xs text-zinc-600">
                          {formatBytes(m.size_bytes)}
                        </span>
                        {active && m.downloaded && (
                          <span className="text-xs text-blue-400">Selected</span>
                        )}
                      </div>
                      <p className="text-xs text-zinc-500 mt-0.5">{m.note}</p>
                    </button>

                    {downloading ? (
                      <span className="text-xs text-zinc-400 tabular-nums">{pct}%</span>
                    ) : m.downloaded ? (
                      <button
                        type="button"
                        onClick={() => remove(m.id)}
                        className="text-xs text-zinc-500 hover:text-red-400 transition-colors"
                      >
                        Remove
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => download(m.id)}
                        className="text-xs px-2.5 py-1 rounded-md bg-zinc-800 text-zinc-200 hover:bg-zinc-700 transition-colors"
                      >
                        Download
                      </button>
                    )}
                  </div>

                  {downloading && (
                    <div className="mt-2 h-1 rounded-full bg-zinc-800 overflow-hidden">
                      <div
                        className="h-full bg-blue-500 transition-all duration-200"
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          <Field label="Formatting">
            <select {...register("formatter")} className="input">
              {formatters.map((f) => (
                <option key={f.value} value={f.value}>{f.label}</option>
              ))}
            </select>
          </Field>

          {formatter === "ollama" && (
            <>
              <Field label="Ollama URL">
                <input
                  {...register("ollamaUrl")}
                  placeholder="http://localhost:11434"
                  className="input"
                />
              </Field>
              <Field label="Ollama Model">
                <input
                  {...register("ollamaModel")}
                  placeholder="qwen3:4b"
                  className="input"
                />
              </Field>
            </>
          )}
        </div>
      )}

      {mode === "cloud" && (
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
      )}

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

const ModelStatusBadge = ({ status }: { status: ModelStatus }) => {
  const variants: Record<ModelStatus, { label: string; className: string }> = {
    ready: { label: "Ready", className: "bg-green-500/15 text-green-400" },
    loading: { label: "Loading…", className: "bg-blue-500/15 text-blue-400" },
    failed: { label: "Failed to load", className: "bg-red-500/15 text-red-400" },
    missing: { label: "No model", className: "bg-zinc-700/40 text-zinc-400" },
  };
  const { label, className } = variants[status];

  return (
    <span className={cn("text-xs px-2 py-0.5 rounded-full font-medium", className)}>
      {label}
    </span>
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
