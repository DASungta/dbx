import assert from "node:assert/strict";
import { test } from "vitest";
import { buildDeploySqlForObjects, convertToSchemaDiffObjects, detectDestructiveSchemaDiffStatements, groupDiffObjects, schemaDiffDeployTargetSchema, setSchemaDiffObjectSelected, summarizeSchemaDiffOperations, type TableDiff } from "../../apps/desktop/src/lib/schema/schemaDiff.ts";

test("uses generated sync SQL for modified table deployment", () => {
  const tableDiffs: TableDiff[] = [
    {
      type: "modified",
      objectType: "table",
      name: "users",
      ddl: "CREATE TABLE `users` (`name` varchar(64));",
      syncSql: "-- Alter table: users\nALTER TABLE `users`\n  MODIFY COLUMN `name` varchar(128) NOT NULL;",
      columns: [
        {
          type: "modified",
          name: "name",
          changes: ["type: varchar(64) -> varchar(128)"],
        },
      ],
    },
  ];

  const objects = convertToSchemaDiffObjects(tableDiffs);
  const deploySql = buildDeploySqlForObjects(objects);

  assert.equal(deploySql, "-- Alter table: users\nALTER TABLE `users`\n  MODIFY COLUMN `name` varchar(128) NOT NULL;\n");
  assert.equal(deploySql.includes("CREATE TABLE"), false);
});

test("falls back to source DDL when object sync SQL is unavailable", () => {
  const tableDiffs: TableDiff[] = [
    {
      type: "added",
      objectType: "table",
      name: "users",
      ddl: "CREATE TABLE `users` (`id` int);",
    },
  ];

  const objects = convertToSchemaDiffObjects(tableDiffs);

  assert.equal(buildDeploySqlForObjects(objects), "-- Create table: users\nCREATE TABLE `users` (`id` int);\n");
});

test("uses mysql target database as schema diff deploy qualifier", () => {
  assert.equal(schemaDiffDeployTargetSchema("mysql", "target_db", ""), "target_db");
  assert.equal(schemaDiffDeployTargetSchema("mysql", "target_db", "  "), "target_db");
  assert.equal(schemaDiffDeployTargetSchema("mysql", "target_db", "explicit_schema"), "explicit_schema");
  assert.equal(schemaDiffDeployTargetSchema("sqlite", "main", ""), undefined);
});

test("counts a removed index in the delete group when its table is modified", () => {
  const objects = convertToSchemaDiffObjects([
    {
      type: "modified",
      objectType: "table",
      name: "users",
      syncSql: "DROP INDEX `idx_users_email` ON `users`;",
      indexes: [{ type: "removed", name: "idx_users_email" }],
    },
  ]);

  const deleteGroup = groupDiffObjects(objects).find((group) => group.operationType === "delete");
  const indexes = deleteGroup?.typeGroups.find((group) => group.kind === "index")?.objects ?? [];

  assert.deepEqual(
    indexes.map((index) => ({ name: index.name, parentName: index.parentName })),
    [{ name: "idx_users_email", parentName: "users" }],
  );
  assert.equal(deleteGroup?.count, 1);
  assert.equal(summarizeSchemaDiffOperations(objects).delete, 1);
});

test("surfaces a modified index as delete risk because deployment drops it first", () => {
  const objects = convertToSchemaDiffObjects([
    {
      type: "modified",
      objectType: "table",
      name: "users",
      syncSql: "DROP INDEX `idx_users_email` ON `users`;\nCREATE UNIQUE INDEX `idx_users_email` ON `users` (`email`);",
      indexes: [{ type: "modified", name: "idx_users_email", changes: ["unique: NO -> YES"] }],
    },
  ]);

  const index = objects[0].children?.find((child) => child.objectKind === "index");
  assert.equal(index?.operationType, "delete");
  assert.equal(summarizeSchemaDiffOperations(objects).delete, 1);
});

test("surfaces a modified foreign key as delete risk because deployment drops it first", () => {
  const objects = convertToSchemaDiffObjects([
    {
      type: "modified",
      objectType: "table",
      name: "orders",
      syncSql: "ALTER TABLE `orders` DROP FOREIGN KEY `fk_orders_user`;",
      foreignKeys: [{ type: "modified", name: "fk_orders_user", changes: ["onDelete: RESTRICT -> CASCADE"] }],
    },
  ]);

  const foreignKey = objects[0].children?.find((child) => child.objectKind === "foreignKey");
  assert.equal(foreignKey?.operationType, "delete");
  assert.equal(summarizeSchemaDiffOperations(objects).delete, 1);
});

test("does not double count children when an entire table is deleted", () => {
  const objects = convertToSchemaDiffObjects([
    {
      type: "removed",
      objectType: "table",
      name: "legacy_users",
      columns: [{ type: "removed", name: "id" }],
      indexes: [{ type: "removed", name: "idx_legacy_users_id" }],
    },
  ]);

  const counts = summarizeSchemaDiffOperations(objects);
  const deleteGroup = groupDiffObjects(objects).find((group) => group.operationType === "delete");
  assert.equal(counts.delete, 1);
  assert.equal(deleteGroup?.count, 1);
  assert.deepEqual(
    deleteGroup?.typeGroups.map((group) => group.kind),
    ["table"],
  );
});

test("clearing an index clears its atomic table deploy unit", () => {
  const objects = convertToSchemaDiffObjects([
    {
      type: "modified",
      objectType: "table",
      name: "users",
      syncSql: "ALTER TABLE `users` DROP INDEX `idx_users_email`, ADD COLUMN `nickname` varchar(64);",
      columns: [{ type: "added", name: "nickname" }],
      indexes: [{ type: "removed", name: "idx_users_email" }],
    },
  ]);

  assert.equal(setSchemaDiffObjectSelected(objects, "idx-users-idx_users_email", false), true);
  assert.equal(objects[0].selected, false);
  assert.equal(
    objects[0].children?.every((child) => !child.selected),
    true,
  );
  assert.equal(buildDeploySqlForObjects(objects), "-- No objects selected");
});

test("detects destructive schema diff statements without comment or string false positives", () => {
  const destructive = detectDestructiveSchemaDiffStatements(
    ["-- DROP TABLE audit_log", "SELECT 'DROP INDEX idx_fake' AS message", 'ALTER TABLE "DROP INDEX audit" ADD COLUMN note text', "DROP INDEX IF EXISTS idx_users_email", "ALTER TABLE users DROP COLUMN legacy_code, DROP INDEX idx_legacy"].join(";\n"),
    "postgres",
  );

  assert.deepEqual(
    destructive.map(({ objectType }) => objectType),
    ["INDEX", "COLUMN", "INDEX"],
  );
});
