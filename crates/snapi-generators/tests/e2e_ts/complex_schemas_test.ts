/**
 * E2E tests for complex_schemas.yaml, exercising:
 *  - allOf merging (Article = Timestamped + inline object fields)
 *  - Inline unnamed objects (Article.author field, addTreeChild body)
 *  - oneOf union response type (SearchResult = ArticleResult | UserResult)
 *  - additionalProperties map field (Config.values: Record<string,string>)
 *  - Nullable field via oneOf [string, null] (Config.description)
 *  - Recursive circular refs (TreeNode.children → TreeNode, TreeNode.parent → TreeNode|null)
 *  - Nested oneOf + allOf (NotificationPayload = union of two allOf-merged objects)
 *  - allOf inheritance (Notification = allOf[BaseNotification, {sent_at}])
 *  - Query param name mapping: snake_case spec → camelCase TS → original name in URL
 */
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
  const BASE = "https://complex.example.com";
  const client = new ApiClient({ baseUrl: BASE });

  // -----------------------------------------------------------------------
  // allOf merging: Article has fields from Timestamped (created_at, updated_at)
  // merged with inline object fields (id, title, body, tags, author).
  // author is itself an inline unnamed object { id?, name? }.
  // -----------------------------------------------------------------------

  // listArticles — plain GET, allOf-merged response
  mockFetch([
    {
      created_at: "2024-01-01T00:00:00Z",
      updated_at: "2024-06-01T00:00:00Z",
      id: "art-1",
      title: "Hello World",
      body: "First article",
      tags: ["typescript", "api"],
      author: { id: "u-1", name: "Alice" },
    },
  ]);
  const articles = await client.articles.listArticles();
  eq(calls.at(-1)!.url, `${BASE}/articles`);
  eq(calls.at(-1)!.method, "GET");
  // allOf fields from Timestamped are present on the merged Article
  eq(articles[0].created_at, "2024-01-01T00:00:00Z");
  eq(articles[0].updated_at, "2024-06-01T00:00:00Z");
  // allOf fields from the inline object extension
  eq(articles[0].title, "Hello World");
  eq(articles[0].tags![0], "typescript");
  // inline unnamed object field (Article.author): { id?: string; name?: string }
  eq(articles[0].author!.name, "Alice");
  eq(articles[0].author!.id, "u-1");

  // listArticles + query params — snake_case spec name maps to camelCase TS key,
  // but the URL param must use the original spec name (author_id, not authorId)
  mockFetch([]);
  await client.articles.listArticles({ authorId: "u-42", limit: 3 });
  ok(
    calls.at(-1)!.url.includes("author_id=u-42"),
    `Expected author_id=u-42 in URL, got: ${calls.at(-1)!.url}`
  );
  ok(
    calls.at(-1)!.url.includes("limit=3"),
    `Expected limit=3 in URL, got: ${calls.at(-1)!.url}`
  );

  // createArticle — POST with named ref body (CreateArticleInput)
  mockFetch({
    created_at: "2024-07-01T00:00:00Z",
    id: "art-2",
    title: "New Post",
    body: "Content here",
    tags: ["rust"],
  });
  const created = await client.articles.createArticle({
    title: "New Post",
    body: "Content here",
    tags: ["rust"],
  });
  eq(calls.at(-1)!.method, "POST");
  eq(calls.at(-1)!.url, `${BASE}/articles`);
  eq(calls.at(-1)!.body!["title"], "New Post");
  eq((calls.at(-1)!.body!["tags"] as unknown[])[0], "rust");
  eq(created.id, "art-2");

  // getArticle — path param, allOf merged response
  mockFetch({
    created_at: "2024-01-01T00:00:00Z",
    id: "art-1",
    title: "Hello World",
    body: "First article",
  });
  const article = await client.articles.getArticle("art-1");
  eq(calls.at(-1)!.url, `${BASE}/articles/art-1`);
  eq(calls.at(-1)!.method, "GET");
  eq(article.id, "art-1");

  // -----------------------------------------------------------------------
  // oneOf union response: SearchResult = ArticleResult | UserResult
  // -----------------------------------------------------------------------

  // Article variant
  mockFetch([{ kind: "article", id: "art-1", title: "Hello World" }]);
  const results = await client.search.search({ q: "hello" });
  ok(
    calls.at(-1)!.url.includes("q=hello"),
    `Expected q=hello in URL, got: ${calls.at(-1)!.url}`
  );
  // kind and id are common to both oneOf variants
  eq(results[0].kind, "article");
  eq(results[0].id, "art-1");
  // title is specific to ArticleResult — narrow to access it
  eq((results[0] as { title: string }).title, "Hello World");

  // User variant of the oneOf union
  mockFetch([{ kind: "user", id: "u-5", username: "bob" }]);
  const results2 = await client.search.search({ q: "bob", kind: "user" });
  ok(
    calls.at(-1)!.url.includes("q=bob"),
    `Expected q=bob, got: ${calls.at(-1)!.url}`
  );
  ok(
    calls.at(-1)!.url.includes("kind=user"),
    `Expected kind=user, got: ${calls.at(-1)!.url}`
  );
  // username is specific to UserResult — narrow to access it
  eq((results2[0] as { username: string }).username, "bob");

  // -----------------------------------------------------------------------
  // additionalProperties map (Config.values: Record<string,string>)
  // + nullable field via oneOf [string, null] (Config.description)
  // -----------------------------------------------------------------------

  // listConfigs — map field present in response
  mockFetch([
    { name: "theme", values: { color: "blue", font: "sans" }, description: null },
    { name: "limits", values: { max_requests: "1000" }, description: "Rate limits" },
  ]);
  const configs = await client.configs.listConfigs();
  eq(calls.at(-1)!.url, `${BASE}/configs`);
  eq(configs[0].values["color"], "blue");
  eq(configs[0].values["font"], "sans");
  // nullable description: null value
  ok(configs[0].description === null, "description should be null");
  // non-null description
  eq(configs[1].description, "Rate limits");

  // updateConfig — PUT with path param and named-ref body (Config); map field in body
  mockFetch({ name: "theme", values: { color: "red" }, description: "Updated theme" });
  const updatedConfig = await client.configs.updateConfig("theme", {
    name: "theme",
    values: { color: "red" },
    description: "Updated theme",
  });
  eq(calls.at(-1)!.method, "PUT");
  eq(calls.at(-1)!.url, `${BASE}/configs/theme`);
  // body contains the map field
  eq((calls.at(-1)!.body!["values"] as Record<string, unknown>)["color"], "red");
  eq(calls.at(-1)!.body!["description"], "Updated theme");
  eq(updatedConfig.values["color"], "red");

  // -----------------------------------------------------------------------
  // Recursive circular refs: TreeNode.children → TreeNode[], parent → TreeNode|null
  // -----------------------------------------------------------------------

  // getTree — response with circular children array + null parent
  mockFetch({
    id: "root",
    label: "Root",
    children: [
      {
        id: "child-1",
        label: "Child A",
        children: [{ id: "grandchild-1", label: "Grandchild", children: [] }],
      },
    ],
    parent: null,
  });
  const tree = await client.trees.getTree("root");
  eq(calls.at(-1)!.url, `${BASE}/trees/root`);
  eq(calls.at(-1)!.method, "GET");
  eq(tree.id, "root");
  // children array (recursive)
  eq(tree.children![0].label, "Child A");
  // nested children two levels deep
  eq(tree.children![0].children![0].label, "Grandchild");
  // nullable parent
  ok(tree.parent === null, "parent should be null");

  // addTreeChild — POST with inline unnamed object body
  // body type: { label: string; metadata?: Record<string, string> }
  // This is NOT a $ref — it is rendered as an inline object type in the method signature.
  mockFetch({ id: "child-2", label: "Child B", children: [], parent: null });
  const child = await client.trees.addTreeChild("root", {
    label: "Child B",
    metadata: { source: "import", priority: "high" },
  });
  eq(calls.at(-1)!.method, "POST");
  eq(calls.at(-1)!.url, `${BASE}/trees/root`);
  // inline body fields
  eq(calls.at(-1)!.body!["label"], "Child B");
  // additionalProperties map in the inline body
  eq((calls.at(-1)!.body!["metadata"] as Record<string, unknown>)["source"], "import");
  eq((calls.at(-1)!.body!["metadata"] as Record<string, unknown>)["priority"], "high");
  eq(child.id, "child-2");

  // addTreeChild without optional metadata field
  mockFetch({ id: "child-3", label: "Child C", children: [] });
  await client.trees.addTreeChild("root", { label: "Child C" });
  eq(calls.at(-1)!.body!["label"], "Child C");
  ok(
    calls.at(-1)!.body!["metadata"] === undefined,
    "metadata should be absent when not passed"
  );

  // -----------------------------------------------------------------------
  // Nested oneOf + allOf: NotificationPayload = union of two allOf-merged objects.
  // Each variant is allOf[BaseNotification, {email}] or allOf[BaseNotification, {phone}].
  // The type renders as the named alias NotificationPayload in the method signature.
  // allOf inheritance: Notification = allOf[BaseNotification, {sent_at}].
  // -----------------------------------------------------------------------

  // Email variant (first oneOf branch)
  mockFetch({
    id: "notif-1",
    channel: "email",
    sent_at: "2024-09-01T12:00:00Z",
  });
  const notif1 = await client.notifications.sendNotification({
    id: "notif-1",
    channel: "email",
    email: "alice@example.com",
  });
  eq(calls.at(-1)!.method, "POST");
  eq(calls.at(-1)!.url, `${BASE}/notifications`);
  // body has the BaseNotification fields + email (allOf branch 1)
  eq(calls.at(-1)!.body!["id"], "notif-1");
  eq(calls.at(-1)!.body!["channel"], "email");
  eq(calls.at(-1)!.body!["email"], "alice@example.com");
  // response is Notification (allOf[BaseNotification, {sent_at}]) — all merged fields present
  eq(notif1.id, "notif-1");
  eq(notif1.channel, "email");
  eq(notif1.sent_at, "2024-09-01T12:00:00Z");

  // SMS variant (second oneOf branch)
  mockFetch({
    id: "notif-2",
    channel: "sms",
    sent_at: "2024-09-01T12:01:00Z",
  });
  const notif2 = await client.notifications.sendNotification({
    id: "notif-2",
    channel: "sms",
    phone: "+1-555-0100",
  });
  eq(calls.at(-1)!.body!["phone"], "+1-555-0100");
  // channel field comes from the BaseNotification allOf entry in both branches
  eq(calls.at(-1)!.body!["channel"], "sms");
  eq(notif2.id, "notif-2");

  console.log("✓ All complex_schemas e2e tests passed");
})().catch((e) => {
  console.error("✗", e.message);
  process.exit(1);
});
