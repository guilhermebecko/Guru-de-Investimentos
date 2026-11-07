import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { createRequire } from "node:module";
const { Core } = createRequire(import.meta.url)("../index.js");

const pastaFixtures = resolve(import.meta.dirname, "../../guru-core/tests/fixtures");
const abrir = () =>
  Core.open(mkdtempSync(join(tmpdir(), "guru-")), { pastaFixtures, hoje: "2026-10-28" });

const cdb = {
  tipo: "rf-cdb",
  instituicao: "Banco Inter",
  emissor: "Banco Inter",
  indexador: "pos",
  taxa: "100",
  benchmark: "cdi",
  dataAplicacao: "2026-08-31",
  valorAplicadoCentavos: 1000000,
  liquidez: "diaria",
  ir: "regressiva",
};

test("statusMercado lista as seis fontes com rótulos, ainda sem dados", () => {
  const status = abrir().statusMercado();
  assert.deepEqual(
    status.map((s) => [s.fonte, s.rotulo]),
    [
      ["cdi", "CDI"],
      ["selic", "Selic"],
      ["ipca", "IPCA"],
      ["tesouro", "Tesouro Direto"],
      ["b3", "Cotações da B3"],
      ["cvm", "Cotas de fundos (CVM)"],
    ],
  );
  assert.equal(status[0].ultimaData, undefined);
});

test("atualizarMercado resolve com 6 fontes e o CDB passa a ter valor calculado", async () => {
  const core = abrir();
  const r = await core.atualizarMercado();
  assert.equal(r.fontes.length, 6);
  assert.ok(r.novos > 0);
  assert.equal(r.fontes[0].ultimaData, "2026-10-28");
  const criado = core.criarInvestimento(cdb);
  assert.deepEqual(criado.erros, []);
  const i = core.investimento(criado.id);
  assert.equal(i.origemValor, "calculo");
  assert.equal(i.origemValorRotulo, "calculado pelo índice");
  assert.equal(i.dataValor, "2026-10-28");
  assert.ok(i.valorAtualCentavos > i.valorAplicadoCentavos);
  assert.ok(i.rentabilidadeDecimos > 0);
  assert.equal(core.resumoCarteira().totalAtualCentavos, i.valorAtualCentavos);
  assert.equal(core.listarInvestimentos()[0].valorAtualCentavos, i.valorAtualCentavos);
  assert.equal(core.statusMercado()[0].ultimaData, "2026-10-28");
});

test("atualizar de novo não grava linhas repetidas", async () => {
  const core = abrir();
  await core.atualizarMercado();
  const segunda = await core.atualizarMercado();
  assert.equal(segunda.novos, 0);
});

test("hoje inválido, ou sem pastaFixtures, é recusado", () => {
  const dir = mkdtempSync(join(tmpdir(), "guru-"));
  assert.throws(() => Core.open(dir, { pastaFixtures, hoje: "28/10/2026" }), {
    code: "MERCADO_CONFIG_INVALIDA",
  });
  assert.throws(() => Core.open(dir, { hoje: "2026-10-28" }), {
    code: "MERCADO_CONFIG_INVALIDA",
  });
});

test("falha de banco na atualização rejeita com o code do núcleo", async () => {
  const dir = mkdtempSync(join(tmpdir(), "guru-"));
  const core = Core.open(dir, { pastaFixtures, hoje: "2026-10-28" });
  // Outra conexão marca o banco como de versão mais nova, sem apagar arquivos abertos:
  // a conexão própria da tarefa (Db::open) passa a falhar.
  const outra = new DatabaseSync(join(dir, "guru.db"));
  outra.exec("PRAGMA user_version = 99");
  outra.close();
  await assert.rejects(core.atualizarMercado(), { code: "SCHEMA_TOO_NEW" });
});

test("pastaFixtures vazia é recusada", () => {
  const dir = mkdtempSync(join(tmpdir(), "guru-"));
  assert.throws(() => Core.open(dir, { pastaFixtures: "" }), {
    code: "MERCADO_CONFIG_INVALIDA",
  });
});

test("com hoje fixo, gravar com a data real continua valendo; o valor usa a data de mercado", async () => {
  const core = abrir();
  await core.atualizarMercado();
  const d = new Date();
  const real = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const r = core.criarInvestimento({ ...cdb, dataAplicacao: real });
  assert.deepEqual(r.erros, []);
  const i = core.investimento(r.id);
  assert.equal(i.valorAtualCentavos, i.valorAplicadoCentavos);
  assert.equal(i.origemValor, "sem_dados");
  const meta = core.criarMeta({ nome: "Viagem", valorAlvoCentavos: 1000000, prazo: `${d.getFullYear() + 5}-12` });
  assert.deepEqual(meta.erros, []);
});
