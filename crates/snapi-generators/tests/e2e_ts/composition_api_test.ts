import { ApiClient } from "./src/client";

type Call = {
  url: string;
  method: string;
  body?: unknown;
  headers: Record<string, string>;
};
const calls: Call[] = [];

function mockFetch(data: unknown, status = 200) {
  (globalThis as Record<string, unknown>).fetch = async (
    url: string | URL,
    opts?: RequestInit
  ) => {
    const rawBody = opts?.body;
    calls.push({
      url: String(url),
      method: (opts?.method ?? "GET").toUpperCase(),
      body: typeof rawBody === "string" ? JSON.parse(rawBody) : undefined,
      headers: (opts?.headers ?? {}) as Record<string, string>,
    });
    return {
      ok: status >= 200 && status < 300,
      status,
      json: async () => data,
    } as unknown as Response;
  };
}

function eq<T>(a: T, b: T, msg?: string) {
  if (a !== b) {
    throw new Error(
      msg ?? `Expected ${JSON.stringify(a)} === ${JSON.stringify(b)}`
    );
  }
}

function ok(v: boolean, msg?: string) {
  if (!v) throw new Error(msg ?? "Expected truthy value");
}

(async () => {
  const BASE = "https://rtc.fishjam.io";
  const client = new ApiClient({ baseUrl: BASE, apiKey: "secret-token" });

  // createComposition - POST /api/composition with body
  mockFetch({ composition_id: "comp-1", api_url: `${BASE}/api/composition/comp-1` }, 201);
  const created = await client.composition.createComposition({ autostart: true });
  eq(created.composition_id, "comp-1");
  eq(created.api_url, `${BASE}/api/composition/comp-1`);
  eq(calls.at(-1)!.url, `${BASE}/api/composition`);
  eq(calls.at(-1)!.method, "POST");

  // createComposition - autostart field is sent in body
  mockFetch({ composition_id: "comp-2", api_url: `${BASE}/api/composition/comp-2` }, 201);
  await client.composition.createComposition({ autostart: false });
  eq((calls.at(-1)!.body as Record<string, unknown>)["autostart"], false);

  // deleteComposition - DELETE with path param, no response body
  mockFetch(null, 200);
  await client.composition.deleteComposition("comp-1");
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1`);
  eq(calls.at(-1)!.method, "DELETE");

  // start - POST with one path param, no request body
  mockFetch({});
  await client.controlRequest.start("comp-1");
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1/start`);
  eq(calls.at(-1)!.method, "POST");
  ok(calls.at(-1)!.body === undefined, "start should have no request body");

  // reset - POST with one path param, no request body
  mockFetch({});
  await client.controlRequest.reset("comp-1");
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1/reset`);
  eq(calls.at(-1)!.method, "POST");
  ok(calls.at(-1)!.body === undefined, "reset should have no request body");

  // registerInput - POST with two path params and JSON body
  mockFetch({ port: 5004 });
  await client.registerRequest.registerInput("comp-1", "input-1", {
    type: "rtp_stream",
    port: 5004,
  });
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1/input/input-1/register`);
  eq(calls.at(-1)!.method, "POST");
  eq((calls.at(-1)!.body as Record<string, unknown>)["type"], "rtp_stream");
  eq((calls.at(-1)!.body as Record<string, unknown>)["port"], 5004);

  // registerOutput - POST with two path params and JSON body
  mockFetch({});
  await client.registerRequest.registerOutput("comp-1", "output-1", {
    type: "mp4",
    path: "/tmp/output.mp4",
  });
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1/output/output-1/register`);
  eq(calls.at(-1)!.method, "POST");
  eq((calls.at(-1)!.body as Record<string, unknown>)["type"], "mp4");

  // updateOutput - POST with two path params and JSON body
  mockFetch({});
  await client.updateRequest.updateOutput("comp-1", "output-1", {
    schedule_time_ms: 1000,
  });
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1/output/output-1/update`);
  eq(calls.at(-1)!.method, "POST");
  eq((calls.at(-1)!.body as Record<string, unknown>)["schedule_time_ms"], 1000);

  // requestKeyframe - POST with two path params, no request body
  mockFetch({});
  await client.updateRequest.requestKeyframe("comp-1", "output-1");
  eq(calls.at(-1)!.url, `${BASE}/api/composition/comp-1/output/output-1/request_keyframe`);
  eq(calls.at(-1)!.method, "POST");
  ok(calls.at(-1)!.body === undefined, "requestKeyframe should have no request body");

  // Bearer token forwarded on all requests
  for (const call of calls) {
    eq(
      call.headers["Authorization"],
      "Bearer secret-token",
      `Expected Authorization header on ${call.method} ${call.url}`
    );
  }

  console.log("✓ All composition_api e2e tests passed");
})().catch((e) => {
  console.error("✗", e.message);
  process.exit(1);
});
