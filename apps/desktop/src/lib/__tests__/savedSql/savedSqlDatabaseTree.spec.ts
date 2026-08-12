import { describe, expect, it } from "vitest";
import { buildDatabaseSavedSqlRootNode, decorateDatabaseSavedSqlTreeNodes, savedSqlFilesForDatabase, stripDatabaseSavedSqlTreeNodes } from "@/lib/savedSql/savedSqlDatabaseTree";
import type { SavedSqlFile, TreeNode } from "@/types/database";

function file(input: Partial<SavedSqlFile> & Pick<SavedSqlFile, "id" | "name" | "connectionId" | "database">): SavedSqlFile {
  return {
    sql: "select 1",
    createdAt: "2026-08-12T00:00:00.000Z",
    updatedAt: "2026-08-12T00:00:00.000Z",
    ...input,
  };
}

const files = [
  file({ id: "orders-10", name: "orders-10.sql", connectionId: "conn-1", database: "app", folderId: "reports" }),
  file({ id: "orders-2", name: "orders-2.sql", connectionId: "conn-1", database: "app" }),
  file({ id: "analytics", name: "analytics.sql", connectionId: "conn-1", database: "analytics" }),
  file({ id: "other-connection", name: "orders.sql", connectionId: "conn-2", database: "app" }),
];

describe("savedSqlFilesForDatabase", () => {
  it("filters by the persisted connection and database association across all SQL library folders", () => {
    expect(savedSqlFilesForDatabase(files, { connectionId: "conn-1", database: "app" }).map((item) => item.id)).toEqual(["orders-2", "orders-10"]);
  });

  it("keeps each original SQL before its naturally ordered copies", () => {
    const copiedFiles = [
      file({ id: "query-5-copy-10", name: "query_5_copy10.sql", connectionId: "conn-1", database: "app" }),
      file({ id: "query-10", name: "query_10.sql", connectionId: "conn-1", database: "app" }),
      file({ id: "query-5-copy-1", name: "query_5_copy1.sql", connectionId: "conn-1", database: "app" }),
      file({ id: "query-2", name: "query_2.sql", connectionId: "conn-1", database: "app" }),
      file({ id: "query-5", name: "query_5.sql", connectionId: "conn-1", database: "app" }),
      file({ id: "query-5-copy-2", name: "query_5_copy2.sql", connectionId: "conn-1", database: "app" }),
    ];

    expect(savedSqlFilesForDatabase(copiedFiles, { connectionId: "conn-1", database: "app" }).map((item) => item.name)).toEqual(["query_2.sql", "query_5.sql", "query_5_copy1.sql", "query_5_copy2.sql", "query_5_copy10.sql", "query_10.sql"]);
  });
});

describe("database saved SQL tree", () => {
  const database: TreeNode = {
    id: "conn-1:app",
    label: "app",
    type: "database",
    connectionId: "conn-1",
    database: "app",
    children: [],
  };

  it("keeps a visible Queries node even when the database has no saved SQL", () => {
    const root = buildDatabaseSavedSqlRootNode(database, []);
    expect(root).toMatchObject({ id: "conn-1:app:__queries", label: "tree.queries", type: "saved-sql-root", children: [] });
  });

  it("adds Queries after database metadata and preserves its expansion state", () => {
    const existingRoot = { ...buildDatabaseSavedSqlRootNode(database, files)!, isExpanded: false };
    const decorated = decorateDatabaseSavedSqlTreeNodes(
      [
        {
          ...database,
          children: [{ id: "conn-1:app:tables", label: "tree.tables", type: "group-tables" }],
        },
      ],
      files,
      [{ ...database, children: [existingRoot] }],
    );

    expect(decorated[0].children?.map((child) => child.type)).toEqual(["group-tables", "saved-sql-root"]);
    expect(decorated[0].children?.at(-1)).toMatchObject({ isExpanded: false, children: [{ savedSqlId: "orders-2" }, { savedSqlId: "orders-10" }] });
  });

  it("removes runtime saved SQL nodes before metadata is cached", () => {
    const decorated = decorateDatabaseSavedSqlTreeNodes([database], files);
    expect(stripDatabaseSavedSqlTreeNodes(decorated)).toEqual([{ ...database, children: [] }]);
  });
});
