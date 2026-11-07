import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRequire } from "node:module";
const { Core } = createRequire(import.meta.url)("../index.js");

test("abre o banco e informa as versões", () => {
  const dir = join(mkdtempSync(join(tmpdir(), "guru-")), "João da Silva");
  const info = Core.open(dir).info();
  assert.equal(info.schemaVersion, 9);
  assert.match(info.coreVersion, /^\d+\.\d+\.\d+/);
  assert.equal(info.dbPath, join(dir, "guru.db"));
});

test("erro do núcleo chega com code", () => {
  const dir = mkdtempSync(join(tmpdir(), "guru-"));
  writeFileSync(join(dir, "guru.db"), "isto não é um banco");
  assert.throws(() => Core.open(dir), { code: "DB_INVALID" });
});
