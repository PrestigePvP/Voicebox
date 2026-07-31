import { handleMeetings } from "./meetings";

export { TranscriptionSession } from "./session";

const checkAuth = (request: Request, env: Env): boolean => {
  const auth = request.headers.get("Authorization");
  const token = auth?.startsWith("Bearer ") ? auth.slice(7) : null;
  return token !== null && token === env.VOICEBOX_TOKEN;
};

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/health" && request.method === "GET") {
      return Response.json({ status: "ok" });
    }

    if (url.pathname === "/ws" && request.method === "GET") {
      if (!checkAuth(request, env)) {
        return Response.json({ error: "auth_failed" }, { status: 401 });
      }

      if (request.headers.get("Upgrade") !== "websocket") {
        return new Response("Expected WebSocket upgrade", { status: 426 });
      }

      const id = env.TRANSCRIPTION_SESSION.newUniqueId();
      const stub = env.TRANSCRIPTION_SESSION.get(id);
      return stub.fetch(request);
    }

    if (url.pathname.startsWith("/meetings/")) {
      if (!checkAuth(request, env)) {
        return Response.json({ error: "auth_failed" }, { status: 401 });
      }
      return handleMeetings(request, env, url);
    }

    return new Response("Not found", { status: 404 });
  },
};
