export { TranscriptionSession } from "./session";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/health" && request.method === "GET") {
      return Response.json({ status: "ok" });
    }

    if (url.pathname === "/ws" && request.method === "GET") {
      const token =
        request.headers.get("X-VoiceBox-Token") ??
        url.searchParams.get("token");
      if (token !== env.VOICEBOX_TOKEN) {
        return Response.json({ error: "auth_failed" }, { status: 401 });
      }

      if (request.headers.get("Upgrade") !== "websocket") {
        return new Response("Expected WebSocket upgrade", { status: 426 });
      }

      const id = env.TRANSCRIPTION_SESSION.newUniqueId();
      const stub = env.TRANSCRIPTION_SESSION.get(id);
      return stub.fetch(request);
    }

    return new Response("Not found", { status: 404 });
  },
};
