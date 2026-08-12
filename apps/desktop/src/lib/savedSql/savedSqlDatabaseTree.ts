import type { SavedSqlFile, TreeNode } from "@/types/database";
import { stripSqlExtension } from "@/lib/savedSql/savedSqlFileName";

const savedSqlNameCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function savedSqlCopySortKey(name: string): { base: string; copyIndex: number } {
  const base = stripSqlExtension(name);
  const copyMatch = base.match(/^(.*)_copy(\d+)$/i);
  if (!copyMatch?.[1]) return { base, copyIndex: 0 };
  return { base: copyMatch[1], copyIndex: Number(copyMatch[2]) };
}

function compareSavedSqlFiles(left: SavedSqlFile, right: SavedSqlFile): number {
  const leftKey = savedSqlCopySortKey(left.name);
  const rightKey = savedSqlCopySortKey(right.name);
  return savedSqlNameCollator.compare(leftKey.base, rightKey.base) || leftKey.copyIndex - rightKey.copyIndex || savedSqlNameCollator.compare(left.name, right.name) || left.id.localeCompare(right.id);
}

export interface SavedSqlDatabaseScope {
  connectionId: string;
  database: string;
}

export function savedSqlFilesForDatabase(files: readonly SavedSqlFile[], scope: SavedSqlDatabaseScope): SavedSqlFile[] {
  return files.filter((file) => file.connectionId === scope.connectionId && file.database === scope.database).sort(compareSavedSqlFiles);
}

function savedSqlFileNode(rootId: string, file: SavedSqlFile): TreeNode {
  return {
    id: `${rootId}:file:${file.id}`,
    label: file.name,
    type: "saved-sql-file",
    connectionId: file.connectionId,
    database: file.database,
    schema: file.schema,
    savedSqlId: file.id,
  };
}

export function buildDatabaseSavedSqlRootNode(databaseNode: Pick<TreeNode, "id" | "connectionId" | "database">, files: readonly SavedSqlFile[], existingRoot?: TreeNode): TreeNode | null {
  if (!databaseNode.connectionId || databaseNode.database === undefined) return null;

  const id = `${databaseNode.id}:__queries`;
  return {
    id,
    label: "tree.queries",
    type: "saved-sql-root",
    connectionId: databaseNode.connectionId,
    database: databaseNode.database,
    isExpanded: existingRoot?.isExpanded ?? true,
    children: savedSqlFilesForDatabase(files, {
      connectionId: databaseNode.connectionId,
      database: databaseNode.database,
    }).map((file) => savedSqlFileNode(id, file)),
  };
}

export function withDatabaseSavedSqlRoot(databaseNode: Pick<TreeNode, "id" | "connectionId" | "database" | "children">, children: readonly TreeNode[], files: readonly SavedSqlFile[]): TreeNode[] {
  const existingRoot = databaseNode.children?.find((child) => child.type === "saved-sql-root");
  const root = buildDatabaseSavedSqlRootNode(databaseNode, files, existingRoot);
  const metadataChildren = children.filter((child) => child.type !== "saved-sql-root");
  return root ? [...metadataChildren, root] : metadataChildren;
}

export function decorateDatabaseSavedSqlTreeNodes(nodes: readonly TreeNode[], files: readonly SavedSqlFile[], existingNodes: readonly TreeNode[] = []): TreeNode[] {
  const existingById = new Map(existingNodes.map((node) => [node.id, node]));
  return nodes.map((node) => {
    const existing = existingById.get(node.id);
    const children = decorateDatabaseSavedSqlTreeNodes(node.children ?? [], files, existing?.children ?? []);
    if (node.type !== "database") {
      return node.children === undefined ? node : { ...node, children };
    }

    const databaseNode = {
      ...node,
      children: existing?.children ?? node.children,
    };
    return {
      ...node,
      children: withDatabaseSavedSqlRoot(databaseNode, children, files),
    };
  });
}

export function stripDatabaseSavedSqlTreeNodes(nodes: readonly TreeNode[]): TreeNode[] {
  return nodes.flatMap((node) => {
    if (node.type === "saved-sql-root" || node.type === "saved-sql-file" || node.type === "saved-sql-folder") return [];
    const children = node.children ? stripDatabaseSavedSqlTreeNodes(node.children) : undefined;
    const hiddenChildren = node.hiddenChildren ? stripDatabaseSavedSqlTreeNodes(node.hiddenChildren) : undefined;
    if (children === node.children && hiddenChildren === node.hiddenChildren) return [node];
    return [{ ...node, children, hiddenChildren }];
  });
}
