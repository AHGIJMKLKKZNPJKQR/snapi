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
  const client = new ApiClient({ baseUrl: BASE, apiKey: "my-token" });

  // listTodos - GET /todos
  mockFetch([{ id: "1", title: "Buy groceries", done: false }]);
  const todos = await client.todos.listTodos();
  eq(todos[0].title, "Buy groceries");
  eq(calls.at(-1)!.url, `${BASE}/todos`);
  eq(calls.at(-1)!.method, "GET");

  // listTodos + pagination params
  mockFetch([]);
  await client.todos.listTodos({ page: 2, perPage: 10 });
  ok(
    calls.at(-1)!.url.includes("page=2"),
    `Expected URL to include page=2, got: ${calls.at(-1)!.url}`
  );
  ok(
    calls.at(-1)!.url.includes("per_page=10"),
    `Expected URL to include per_page=10, got: ${calls.at(-1)!.url}`
  );

  // listTodos + status param
  mockFetch([]);
  await client.todos.listTodos({ status: "done" });
  ok(
    calls.at(-1)!.url.includes("status=done"),
    `Expected URL to include status=done, got: ${calls.at(-1)!.url}`
  );

  // createTodo - POST with body, check boolean `done` field in response
  mockFetch({ id: "2", title: "Walk the dog", done: false });
  const created = await client.todos.createTodo({ title: "Walk the dog" });
  eq(created.title, "Walk the dog");
  eq(created.done, false);
  eq(calls.at(-1)!.method, "POST");
  eq(calls.at(-1)!.url, `${BASE}/todos`);
  eq(calls.at(-1)!.body!["title"], "Walk the dog");

  // createTodo - POST with `done: true` in body
  mockFetch({ id: "3", title: "Already done task", done: true });
  const createdDone = await client.todos.createTodo({ title: "Already done task", done: true });
  eq(createdDone.done, true);
  eq(calls.at(-1)!.body!["done"], true);
  eq(calls.at(-1)!.body!["title"], "Already done task");

  // getTodo - path param
  mockFetch({ id: "abc-123", title: "Read a book", done: true });
  const todo = await client.todos.getTodo("abc-123");
  eq(todo.id, "abc-123");
  eq(calls.at(-1)!.url, `${BASE}/todos/abc-123`);
  eq(calls.at(-1)!.method, "GET");

  // updateTodo - PUT with body (both fields)
  mockFetch({ id: "abc-123", title: "Read two books", done: false });
  const updated = await client.todos.updateTodo("abc-123", {
    title: "Read two books",
    done: false,
  });
  eq(updated.title, "Read two books");
  eq(updated.done, false);
  eq(calls.at(-1)!.method, "PUT");
  eq(calls.at(-1)!.url, `${BASE}/todos/abc-123`);
  eq(calls.at(-1)!.body!["title"], "Read two books");
  eq(calls.at(-1)!.body!["done"], false);

  // updateTodo - partial update (only `done` field, no title)
  mockFetch({ id: "abc-123", title: "Read two books", done: true });
  const patched = await client.todos.updateTodo("abc-123", { done: true });
  eq(patched.done, true);
  eq(calls.at(-1)!.body!["done"], true);
  ok(calls.at(-1)!.body!["title"] === undefined, "title should be absent in partial update body");

  // deleteTodo - DELETE /todos/{id}
  mockFetch(null, 204);
  await client.todos.deleteTodo("abc-123");
  eq(calls.at(-1)!.url, `${BASE}/todos/abc-123`);
  eq(calls.at(-1)!.method, "DELETE");

  // Bearer token forwarded on every request
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer my-token",
    "Expected Authorization header to be set"
  );

  // getTodo — response with optional timestamps (created_at / updated_at)
  mockFetch({
    id: "ts-1",
    title: "Timestamped task",
    done: false,
    created_at: "2024-01-15T09:00:00Z",
    updated_at: "2024-03-01T12:00:00Z",
  });
  const tsTodo = await client.todos.getTodo("ts-1");
  eq(tsTodo.created_at, "2024-01-15T09:00:00Z");
  eq(tsTodo.updated_at, "2024-03-01T12:00:00Z");

  // getTodo — response without optional timestamps (fields absent)
  mockFetch({ id: "ts-2", title: "No timestamps", done: true });
  const noTsTodo = await client.todos.getTodo("ts-2");
  eq(noTsTodo.done, true);
  ok(noTsTodo.created_at === undefined, "created_at should be absent");
  ok(noTsTodo.updated_at === undefined, "updated_at should be absent");

  // listTodos — `done` boolean field present on all items
  mockFetch([
    { id: "1", title: "Pending task", done: false },
    { id: "2", title: "Done task", done: true },
  ]);
  const mixed = await client.todos.listTodos();
  eq(mixed[0].done, false);
  eq(mixed[1].done, true);

  // Authorization header forwarded on GET (listTodos), POST (createTodo), PUT (updateTodo)
  mockFetch([]);
  await client.todos.listTodos();
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer my-token",
    "Expected Authorization on GET"
  );

  mockFetch({ id: "auth-1", title: "Auth task", done: false });
  await client.todos.createTodo({ title: "Auth task" });
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer my-token",
    "Expected Authorization on POST"
  );

  mockFetch({ id: "auth-1", title: "Auth task", done: false });
  await client.todos.updateTodo("auth-1", { title: "Auth task" });
  eq(
    calls.at(-1)!.headers["Authorization"],
    "Bearer my-token",
    "Expected Authorization on PUT"
  );

  console.log("✓ All crud_with_auth e2e tests passed");
})().catch((e) => {
  console.error("✗", e.message);
  process.exit(1);
});
