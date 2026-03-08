/**
 * E2E tests for circular_refs.yaml, exercising:
 *  - Recursive self-referential schema (TreeNode.children → TreeNode[])
 *  - Nullable circular ref (TreeNode.parent → TreeNode | null)
 *  - POST with named-ref body (addChild → NewNode)
 *  - Doubly-linked circular schema (Category.parent / Category.children)
 *  - Integer path param (getNode, addChild)
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
  const BASE = "https://tree.example.com";
  const client = new ApiClient({ baseUrl: BASE });

  // listNodes — GET /nodes, returns TreeNode[]
  mockFetch([
    { id: 1, value: "root", children: [], parent: null },
    {
      id: 2,
      value: "branch",
      children: [{ id: 3, value: "leaf", children: [] }],
      parent: null,
    },
  ]);
  const nodes = await client.nodes.listNodes();
  eq(calls.at(-1)!.url, `${BASE}/nodes`);
  eq(calls.at(-1)!.method, "GET");
  eq(nodes[0].id, 1);
  eq(nodes[0].value, "root");
  // recursive children array (TreeNode[])
  eq(nodes[1].children![0].value, "leaf");
  // nullable parent field
  ok(nodes[0].parent === null, "parent should be null");

  // getNode — integer path param
  mockFetch({
    id: 5,
    value: "node-5",
    children: [],
    parent: { id: 1, value: "root", children: [] },
  });
  const node = await client.nodes.getNode(5);
  eq(calls.at(-1)!.url, `${BASE}/nodes/5`);
  eq(calls.at(-1)!.method, "GET");
  eq(node.id, 5);
  eq(node.value, "node-5");
  // non-null parent (circular back-ref)
  ok(node.parent !== null, "parent should not be null");

  // addChild — POST with NewNode body, integer path param
  mockFetch({ id: 6, value: "child-of-5", children: [], parent: null });
  const child = await client.nodes.addChild(5, { value: "child-of-5" });
  eq(calls.at(-1)!.method, "POST");
  eq(calls.at(-1)!.url, `${BASE}/nodes/5`);
  eq(calls.at(-1)!.body!["value"], "child-of-5");
  eq(child.id, 6);
  eq(child.value, "child-of-5");

  // listNodes — 3-level deep nesting (grandchild's child)
  mockFetch([
    {
      id: 10,
      value: "level-1",
      children: [
        {
          id: 11,
          value: "level-2",
          children: [
            {
              id: 12,
              value: "level-3",
              children: [{ id: 13, value: "level-4", children: [] }],
            },
          ],
        },
      ],
      parent: null,
    },
  ]);
  const deep = await client.nodes.listNodes();
  eq(deep[0].value, "level-1");
  eq(deep[0].children![0].value, "level-2");
  eq(deep[0].children![0].children![0].value, "level-3");
  eq(deep[0].children![0].children![0].children![0].value, "level-4");

  // getNode — multiple children at the same level
  mockFetch({
    id: 20,
    value: "parent",
    children: [
      { id: 21, value: "sibling-A", children: [] },
      { id: 22, value: "sibling-B", children: [] },
      { id: 23, value: "sibling-C", children: [] },
    ],
    parent: null,
  });
  const multiChild = await client.nodes.getNode(20);
  eq(multiChild.children!.length, 3);
  eq(multiChild.children![0].value, "sibling-A");
  eq(multiChild.children![2].value, "sibling-C");

  // listCategories — doubly-linked circular refs (Category.parent + Category.children)
  mockFetch([
    {
      id: 10,
      name: "Electronics",
      parent: null,
      children: [{ id: 11, name: "Phones", parent: null, children: [] }],
    },
  ]);
  const categories = await client.categories.listCategories();
  eq(calls.at(-1)!.url, `${BASE}/categories`);
  eq(calls.at(-1)!.method, "GET");
  eq(categories[0].id, 10);
  eq(categories[0].name, "Electronics");
  // nested children (recursive Category[])
  eq(categories[0].children![0].name, "Phones");
  // nullable parent
  ok(categories[0].parent === null, "parent should be null");

  // listCategories — 3-level deep category nesting
  mockFetch([
    {
      id: 1,
      name: "Root",
      parent: null,
      children: [
        {
          id: 2,
          name: "Branch",
          parent: null,
          children: [
            { id: 3, name: "Leaf", parent: null, children: [] },
          ],
        },
      ],
    },
  ]);
  const deepCategories = await client.categories.listCategories();
  eq(deepCategories[0].name, "Root");
  eq(deepCategories[0].children![0].name, "Branch");
  eq(deepCategories[0].children![0].children![0].name, "Leaf");

  // addChild — verify parent field can be non-null (circular back-ref populated)
  mockFetch({
    id: 30,
    value: "new-child",
    children: [],
    parent: { id: 20, value: "parent", children: [] },
  });
  const newChild = await client.nodes.addChild(20, { value: "new-child" });
  eq(newChild.id, 30);
  ok(newChild.parent !== null, "parent back-ref should be set");

  // listNodes — nodes with empty children array (no recursion)
  mockFetch([{ id: 100, value: "lone", children: [] }]);
  const lone = await client.nodes.listNodes();
  eq(lone[0].children!.length, 0);

  console.log("✓ All circular_refs e2e tests passed");
})().catch((e) => {
  console.error("✗", e.message);
  process.exit(1);
});
