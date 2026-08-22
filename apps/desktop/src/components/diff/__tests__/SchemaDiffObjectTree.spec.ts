// @vitest-environment happy-dom

import { createApp, defineComponent, h, nextTick, type App } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import { convertToSchemaDiffObjects, groupDiffObjects, type SchemaDiffObject } from "@/lib/schema/schemaDiff";

vi.mock("vue-i18n", () => ({ useI18n: () => ({ t: (key: string, params?: { selected: number; total: number }) => (params ? `${key}:${params.selected}/${params.total}` : key) }) }));

import SchemaDiffObjectTree from "@/components/diff/SchemaDiffObjectTree.vue";

const mountedApps: App[] = [];

afterEach(() => {
  for (const app of mountedApps.splice(0)) app.unmount();
  document.body.innerHTML = "";
});

describe("SchemaDiffObjectTree", () => {
  it("renders operation -> table -> field/index selection without a database root", async () => {
    const groups = groupDiffObjects(
      convertToSchemaDiffObjects([
        {
          type: "modified",
          objectType: "table",
          name: "users",
          columns: [{ type: "added", name: "nickname" }],
          indexes: [{ type: "removed", name: "idx_legacy" }],
        },
      ]),
    );
    const toggles: Array<{ object: SchemaDiffObject; selected: boolean }> = [];
    const host = document.createElement("div");
    document.body.append(host);

    const app = createApp(
      defineComponent({
        setup() {
          return () =>
            h(SchemaDiffObjectTree, {
              groups,
              selectedObjectId: null,
              onToggleObjectSelection: (object: SchemaDiffObject, selected: boolean) => toggles.push({ object, selected }),
            });
        },
      }),
    );
    mountedApps.push(app);
    app.mount(host);
    await nextTick();

    expect(host.textContent).toContain("diff.operationLabel.create");
    expect(host.textContent).toContain("diff.operationLabel.delete");
    expect(host.textContent).not.toContain("database");

    const createTableName = Array.from(host.querySelectorAll("span")).find((element) => element.textContent === "users");
    const createTableRow = createTableName?.closest(".grid") as HTMLElement;
    (createTableRow.querySelector("button") as HTMLButtonElement).click();
    await nextTick();

    const nickname = Array.from(host.querySelectorAll("span")).find((element) => element.textContent === "nickname");
    expect(nickname).toBeTruthy();
    const nicknameCheckbox = nickname?.closest(".grid")?.querySelector('input[type="checkbox"]') as HTMLInputElement;
    nicknameCheckbox.checked = false;
    nicknameCheckbox.dispatchEvent(new Event("change", { bubbles: true }));

    expect(toggles.at(-1)?.object.id).toBe("col-users-nickname");
    expect(toggles.at(-1)?.selected).toBe(false);
  });
});
