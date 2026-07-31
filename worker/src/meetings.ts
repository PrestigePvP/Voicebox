import { novaResponseToTurns, type NovaResponse } from "./diarize";
import {
  buildEnrichMessages,
  emptyEnrichResult,
  parseEnrichResponse,
  sampleTurns,
} from "./meeting-prompt";
import type {
  EnrichRequest,
  EnrichResponse,
  TranscribeRequest,
  TranscribeResponse,
  Turn,
  UploadCompleteRequest,
  UploadCompleteResponse,
  UploadInitRequest,
  UploadInitResponse,
  UploadPartResponse,
} from "./types";

const MAX_UPLOAD_BYTES = 300 * 1024 * 1024;
const KEY_PATTERN = /^m-[0-9a-f-]{36}\.wav$/;

const errorResponse = (error: string, message: string, status: number): Response =>
  Response.json({ error, message }, { status });

const readJson = async <T>(request: Request): Promise<T | null> => {
  try {
    return (await request.json()) as T;
  } catch {
    return null;
  }
};

const uploadInit = async (request: Request, env: Env): Promise<Response> => {
  const body = await readJson<UploadInitRequest>(request);
  if (!body || typeof body.sizeBytes !== "number" || body.sizeBytes <= 0) {
    return errorResponse("bad_request", "Expected { sizeBytes: number }", 400);
  }
  if (body.sizeBytes > MAX_UPLOAD_BYTES) {
    return errorResponse("too_large", `Audio exceeds ${MAX_UPLOAD_BYTES} bytes`, 413);
  }

  const key = `m-${crypto.randomUUID()}.wav`;
  const upload = await env.MEETINGS.createMultipartUpload(key);
  const response: UploadInitResponse = { key, uploadId: upload.uploadId };
  return Response.json(response);
};

const uploadPart = async (request: Request, env: Env, url: URL): Promise<Response> => {
  const key = url.searchParams.get("key") ?? "";
  const uploadId = url.searchParams.get("uploadId") ?? "";
  const partNumber = Number(url.searchParams.get("partNumber"));

  if (!KEY_PATTERN.test(key) || !uploadId || !Number.isInteger(partNumber) || partNumber < 1) {
    return errorResponse("bad_request", "Expected key, uploadId, partNumber query params", 400);
  }
  if (!request.body) {
    return errorResponse("bad_request", "Missing request body", 400);
  }

  const upload = env.MEETINGS.resumeMultipartUpload(key, uploadId);
  try {
    const part = await upload.uploadPart(partNumber, request.body);
    const response: UploadPartResponse = { partNumber: part.partNumber, etag: part.etag };
    return Response.json(response);
  } catch (err) {
    return errorResponse("upload_failed", String(err), 500);
  }
};

const uploadComplete = async (request: Request, env: Env): Promise<Response> => {
  const body = await readJson<UploadCompleteRequest>(request);
  if (!body || !KEY_PATTERN.test(body.key ?? "") || !body.uploadId || !Array.isArray(body.parts)) {
    return errorResponse("bad_request", "Expected { key, uploadId, parts }", 400);
  }

  const upload = env.MEETINGS.resumeMultipartUpload(body.key, body.uploadId);
  try {
    const object = await upload.complete(body.parts);
    const response: UploadCompleteResponse = { sizeBytes: object.size };
    return Response.json(response);
  } catch (err) {
    return errorResponse("upload_failed", String(err), 500);
  }
};

const transcribe = async (request: Request, env: Env): Promise<Response> => {
  const body = await readJson<TranscribeRequest>(request);
  if (!body || !KEY_PATTERN.test(body.key ?? "")) {
    return errorResponse("bad_request", "Expected { key }", 400);
  }

  const object = await env.MEETINGS.get(body.key);
  if (!object) {
    return errorResponse("not_found", `No audio at ${body.key}`, 404);
  }

  const model = env.MEETING_STT_MODEL ?? "@cf/deepgram/nova-3";
  let result: NovaResponse;
  try {
    result = (await env.AI.run(model as Parameters<typeof env.AI.run>[0], {
      audio: { body: object.body, contentType: "audio/wav" },
      diarize: true,
      punctuate: true,
      smart_format: true,
      utterances: true,
    } as never)) as NovaResponse;
  } catch (err) {
    return errorResponse("stt_failed", String(err), 502);
  }

  const turns = novaResponseToTurns(result);
  const durationSec = turns.length > 0 ? turns[turns.length - 1].end : 0;
  const speakerCount = new Set(turns.map((t) => t.speaker)).size;

  const response: TranscribeResponse = { durationSec, model, speakerCount, turns };
  return Response.json(response);
};

const enrich = async (request: Request, env: Env): Promise<Response> => {
  const body = await readJson<EnrichRequest>(request);
  if (!body || !Array.isArray(body.turns) || body.turns.length === 0) {
    return errorResponse("bad_request", "Expected { turns: Turn[] }", 400);
  }

  const turns: Turn[] = body.turns;
  const speakerIds = [...new Set(turns.map((t) => t.speaker))].sort((a, b) => a - b);
  const model = env.MEETING_ENRICH_MODEL ?? env.FORMAT_MODEL ?? "@cf/meta/llama-3.2-3b-instruct";

  const { blocks, sampled } = sampleTurns(turns);
  const messages = buildEnrichMessages(blocks, sampled);

  for (let attempt = 0; attempt < 2; attempt++) {
    let raw: string | null = null;
    try {
      const result = (await env.AI.run(model as Parameters<typeof env.AI.run>[0], {
        messages,
        temperature: 0.2,
      } as never)) as unknown;
      // Chat models return {response: string}, but some auto-parse JSON output
      // and return {response: object}.
      if (typeof result === "string") {
        raw = result;
      } else if (result && typeof result === "object") {
        const response = (result as { response?: unknown }).response;
        if (typeof response === "string") raw = response;
        else if (response && typeof response === "object") raw = JSON.stringify(response);
      }
    } catch (err) {
      console.error("enrich AI call failed:", err);
    }

    if (raw) {
      const parsed = parseEnrichResponse(raw, speakerIds);
      if (parsed) {
        const response: EnrichResponse = { ...parsed, model };
        return Response.json(response);
      }
    }
  }

  const response: EnrichResponse = { ...emptyEnrichResult(speakerIds), model };
  return Response.json(response);
};

export const handleMeetings = async (
  request: Request,
  env: Env,
  url: URL,
): Promise<Response> => {
  const route = `${request.method} ${url.pathname}`;

  switch (route) {
    case "POST /meetings/uploads":
      return uploadInit(request, env);
    case "PUT /meetings/uploads/part":
      return uploadPart(request, env, url);
    case "POST /meetings/uploads/complete":
      return uploadComplete(request, env);
    case "POST /meetings/transcribe":
      return transcribe(request, env);
    case "POST /meetings/enrich":
      return enrich(request, env);
    default:
      return errorResponse("not_found", `No route ${route}`, 404);
  }
};
