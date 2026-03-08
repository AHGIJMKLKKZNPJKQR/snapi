import { ApiClient } from "./src/client";
import type {
  UserCreatedEvent,
  OrderPlacedEvent,
  PaymentProcessedEvent,
} from "./src/models";

type Call = {
  url: string;
  method: string;
  body?: Record<string, unknown>;
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
  const BASE = "https://events.example.com";
  const client = new ApiClient({ baseUrl: BASE });

  // listEvents — GET /events, returns Event[] (oneOf union)
  // `type` is required on all three oneOf variants, so it is accessible on the union directly.
  mockFetch([
    { type: "user_created", user_id: "u-1", email: "alice@example.com", name: "Alice" },
    { type: "order_placed", order_id: "o-1", user_id: "u-1", amount: 99.99, currency: "USD" },
  ]);
  const events = await client.events.listEvents();
  eq(calls.at(-1)!.url, `${BASE}/events`);
  eq(calls.at(-1)!.method, "GET");
  eq(events[0].type, "user_created");
  eq(events[1].type, "order_placed");
  // Narrow to variant-specific fields via imported named types (no `as any` needed)
  eq((events[0] as UserCreatedEvent).user_id, "u-1");
  eq((events[0] as UserCreatedEvent).email, "alice@example.com");
  eq((events[0] as UserCreatedEvent).name, "Alice");
  eq((events[1] as OrderPlacedEvent).order_id, "o-1");
  eq((events[1] as OrderPlacedEvent).amount, 99.99);
  eq((events[1] as OrderPlacedEvent).currency, "USD");

  // listEvents + query params
  mockFetch([]);
  await client.events.listEvents({ since: "2024-01-01T00:00:00Z", limit: 5 });
  ok(
    calls.at(-1)!.url.includes("since="),
    `Expected since param in URL, got: ${calls.at(-1)!.url}`
  );
  ok(
    calls.at(-1)!.url.includes("limit=5"),
    `Expected limit=5 in URL, got: ${calls.at(-1)!.url}`
  );

  // getEvent — path param, PaymentProcessedEvent variant
  // PaymentStatus enum values are lowercase strings (e.g. "succeeded")
  mockFetch({
    type: "payment_processed",
    payment_id: "pay-99",
    order_id: "o-42",
    status: "succeeded",
  });
  const event = await client.events.getEvent("evt-99");
  eq(calls.at(-1)!.url, `${BASE}/events/evt-99`);
  eq(calls.at(-1)!.method, "GET");
  eq(event.type, "payment_processed");
  eq((event as PaymentProcessedEvent).payment_id, "pay-99");
  eq((event as PaymentProcessedEvent).order_id, "o-42");
  // PaymentStatus is a string enum: "pending" | "succeeded" | "failed" | "refunded"
  eq((event as PaymentProcessedEvent).status, "succeeded");

  // getEvent — UserCreatedEvent variant (nullable optional name field)
  mockFetch({
    type: "user_created",
    user_id: "u-2",
    email: "bob@example.com",
  });
  const event2 = await client.events.getEvent("evt-2");
  eq((event2 as UserCreatedEvent).email, "bob@example.com");
  // optional name field absent → undefined
  ok(
    (event2 as UserCreatedEvent).name === undefined,
    "name should be absent"
  );

  // getEvent — OrderPlacedEvent variant
  mockFetch({
    type: "order_placed",
    order_id: "o-77",
    user_id: "u-3",
    amount: 149.95,
    currency: "EUR",
  });
  const orderEvent = await client.events.getEvent("evt-3");
  eq(calls.at(-1)!.url, `${BASE}/events/evt-3`);
  eq(orderEvent.type, "order_placed");
  eq((orderEvent as OrderPlacedEvent).order_id, "o-77");
  eq((orderEvent as OrderPlacedEvent).user_id, "u-3");
  eq((orderEvent as OrderPlacedEvent).amount, 149.95);
  eq((orderEvent as OrderPlacedEvent).currency, "EUR");

  // getEvent — OrderPlacedEvent without optional `currency` field
  mockFetch({
    type: "order_placed",
    order_id: "o-88",
    user_id: "u-4",
    amount: 9.99,
  });
  const orderEventNoCurrency = await client.events.getEvent("evt-4");
  eq((orderEventNoCurrency as OrderPlacedEvent).amount, 9.99);
  ok(
    (orderEventNoCurrency as OrderPlacedEvent).currency === undefined,
    "currency should be absent"
  );

  // PaymentStatus enum — all four values
  for (const [paymentId, status] of [
    ["pay-1", "pending"],
    ["pay-2", "succeeded"],
    ["pay-3", "failed"],
    ["pay-4", "refunded"],
  ] as const) {
    mockFetch({
      type: "payment_processed",
      payment_id: paymentId,
      order_id: "o-100",
      status,
    });
    const payEvt = await client.events.getEvent(`evt-${paymentId}`);
    eq(
      (payEvt as PaymentProcessedEvent).status,
      status,
      `Expected PaymentStatus "${status}"`
    );
  }

  // listEvents — all three event types present in one response
  mockFetch([
    { type: "user_created", user_id: "u-10", email: "charlie@example.com" },
    { type: "order_placed", order_id: "o-10", user_id: "u-10", amount: 50.0 },
    { type: "payment_processed", payment_id: "pay-10", order_id: "o-10", status: "succeeded" },
  ]);
  const allEvents = await client.events.listEvents();
  eq(allEvents.length, 3);
  eq(allEvents[0].type, "user_created");
  eq(allEvents[1].type, "order_placed");
  eq(allEvents[2].type, "payment_processed");
  eq((allEvents[2] as PaymentProcessedEvent).status, "succeeded");

  console.log("✓ All discriminated_union e2e tests passed");
})().catch((e) => {
  console.error("✗", e.message);
  process.exit(1);
});
