import { ApiClient } from "./src/client";

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
  const BASE = "https://api.example.com";
  const client = new ApiClient({ baseUrl: BASE });

  // listPets - GET /pets
  mockFetch([{ id: 1, name: "Whiskers" }]);
  const pets = await client.pets.listPets();
  eq(pets[0].name, "Whiskers");
  eq(calls.at(-1)!.url, `${BASE}/pets`);
  eq(calls.at(-1)!.method, "GET");

  // listPets + query params
  mockFetch([]);
  await client.pets.listPets({ limit: 5 });
  ok(
    calls.at(-1)!.url.includes("limit=5"),
    `Expected URL to include limit=5, got: ${calls.at(-1)!.url}`
  );

  // listPets + multiple query params
  mockFetch([]);
  await client.pets.listPets({ limit: 10, tags: "cat" });
  ok(
    calls.at(-1)!.url.includes("limit=10"),
    `Expected URL to include limit=10, got: ${calls.at(-1)!.url}`
  );
  ok(
    calls.at(-1)!.url.includes("tags=cat"),
    `Expected URL to include tags=cat, got: ${calls.at(-1)!.url}`
  );

  // getPet - path param
  mockFetch({ id: 42, name: "Luna" });
  const pet = await client.pets.getPet(42);
  eq(pet.id, 42);
  eq(calls.at(-1)!.url, `${BASE}/pets/42`);
  eq(calls.at(-1)!.method, "GET");

  // createPet - POST with body
  mockFetch({ id: 99, name: "Rex" });
  const created = await client.pets.createPet({ name: "Rex" });
  eq(created.id, 99);
  eq(calls.at(-1)!.method, "POST");
  eq(calls.at(-1)!.url, `${BASE}/pets`);
  eq(calls.at(-1)!.body!["name"], "Rex");

  // deletePet - DELETE /pets/{id}
  mockFetch(null, 204);
  await client.pets.deletePet(42);
  eq(calls.at(-1)!.url, `${BASE}/pets/42`);
  eq(calls.at(-1)!.method, "DELETE");

  // getPet — response includes optional `tag` field
  mockFetch({ id: 7, name: "Mittens", tag: "indoor" });
  const taggedPet = await client.pets.getPet(7);
  eq(taggedPet.tag, "indoor");
  eq(calls.at(-1)!.url, `${BASE}/pets/7`);

  // getPet — response includes optional `status` field (PetStatus enum)
  mockFetch({ id: 8, name: "Goldie", status: "AVAILABLE" });
  const availablePet = await client.pets.getPet(8);
  eq(availablePet.status, "AVAILABLE");

  mockFetch({ id: 9, name: "Hammy", status: "PENDING" });
  const pendingPet = await client.pets.getPet(9);
  eq(pendingPet.status, "PENDING");

  mockFetch({ id: 10, name: "Scruffy", status: "SOLD" });
  const soldPet = await client.pets.getPet(10);
  eq(soldPet.status, "SOLD");

  // createPet — body includes optional `tag` field
  mockFetch({ id: 11, name: "Fido", tag: "outdoor" });
  const taggedCreated = await client.pets.createPet({ name: "Fido", tag: "outdoor" });
  eq(taggedCreated.tag, "outdoor");
  eq(calls.at(-1)!.body!["name"], "Fido");
  eq(calls.at(-1)!.body!["tag"], "outdoor");

  // listPets — response items with status enum
  mockFetch([
    { id: 1, name: "Rex", status: "AVAILABLE" },
    { id: 2, name: "Fluffy", status: "SOLD" },
  ]);
  const allPets = await client.pets.listPets();
  eq(allPets[0].status, "AVAILABLE");
  eq(allPets[1].status, "SOLD");

  // getPet — response with no optional fields (tag and status absent)
  mockFetch({ id: 12, name: "Bare" });
  const barePet = await client.pets.getPet(12);
  ok(barePet.tag === undefined, "tag should be absent when not in response");
  ok(barePet.status === undefined, "status should be absent when not in response");

  // apiKey → Authorization: Bearer header
  const authClient = new ApiClient({ baseUrl: BASE, apiKey: "tok-123" });
  mockFetch({ id: 1, name: "Buddy" });
  await authClient.pets.getPet(1);
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer tok-123",
    "Expected Authorization header to be set"
  );

  // Authorization header present on POST too
  mockFetch({ id: 99, name: "Auth-pet" });
  await authClient.pets.createPet({ name: "Auth-pet" });
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer tok-123",
    "Expected Authorization header on POST"
  );

  // Authorization header present on DELETE too
  mockFetch(null, 204);
  await authClient.pets.deletePet(1);
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer tok-123",
    "Expected Authorization header on DELETE"
  );

  console.log("✓ All petstore e2e tests passed");
})().catch((e) => {
  console.error("✗", e.message);
  process.exit(1);
});
