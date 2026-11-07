import { test } from "node:test";
import assert from "node:assert/strict";
import { menorData, diaMes, textoIndicador, precisaAtualizar, deveRedesenhar, textoNota, coalescer } from "../../renderer/js/mercado.js";

const fontes = [
  { fonte: "cdi", rotulo: "CDI", ultimaData: "2026-10-28" },
  { fonte: "selic", rotulo: "Selic", ultimaData: "2026-10-27" },
  { fonte: "ipca", rotulo: "IPCA", ultimaData: "2026-09-01" },
  { fonte: "tesouro", rotulo: "Tesouro Direto" },
];

test("menorData ignora cvm", () => {
  assert.equal(
    menorData([
      { fonte: "cdi", ultimaData: "2026-11-06" },
      { fonte: "cvm", ultimaData: "2026-11-04" },
    ]),
    "2026-11-06",
  );
});

test("menorData ignora o IPCA mensal e as fontes sem dados", () => {
  assert.equal(menorData(fontes), "2026-10-27");
  assert.equal(menorData([{ fonte: "ipca", ultimaData: "2026-09-01" }]), null);
  assert.equal(menorData([]), null);
});

test("textos do indicador", () => {
  assert.equal(diaMes("2026-10-28"), "28/10");
  assert.equal(textoIndicador(fontes), "Dados de mercado de 27/10");
  assert.equal(textoIndicador([]), "Dados de mercado: ainda não baixados");
});

test("precisaAtualizar compara com o dia local", () => {
  const agora = new Date(2026, 9, 30, 12, 0, 0);
  const hoje = new Date(2026, 9, 30, 8, 0, 0).toISOString();
  const ontem = new Date(2026, 9, 29, 20, 0, 0).toISOString();
  assert.equal(precisaAtualizar([], agora), true);
  assert.equal(precisaAtualizar([{ atualizadoEm: hoje }, { atualizadoEm: hoje }], agora), false);
  assert.equal(precisaAtualizar([{ atualizadoEm: hoje }, { atualizadoEm: ontem }], agora), true);
  assert.equal(precisaAtualizar([{ atualizadoEm: hoje }, { erro: "falhou" }], agora), true);
  // Fonte que a carteira não usa não força a atualização; sem nenhuma em uso, atualiza.
  assert.equal(precisaAtualizar([{ atualizadoEm: hoje, ultimaData: "2026-10-30" }, { fonte: "tesouro" }], agora), false);
  assert.equal(precisaAtualizar([{ fonte: "tesouro" }, { fonte: "b3" }], agora), true);
});

test("deveRedesenhar só nas telas de leitura", () => {
  for (const h of ["", "#", "#/", "#/carteira", "#/carteira/rf", "#/investimentos", "#/investimentos/"]) {
    assert.equal(deveRedesenhar(h), true, h);
  }
  for (const h of ["#/investimentos/novo", "#/investimentos/abc", "#/perfil", "#/perfil/metas", "#/perfil/alocacao",
    "#/aporte", "#/assistente", "#/ia", "#/xyz"]) {
    assert.equal(deveRedesenhar(h), false, h);
  }
});

test("textoNota", () => {
  assert.equal(textoNota("28/10"), "Valores brutos, antes do IR. Dados de mercado de 28/10.");
  assert.equal(textoNota(null), "Valores brutos, antes do IR. Dados de mercado ainda não baixados.");
});

test("coalescer: pedidos durante a rodada viram uma única rodada seguinte", async () => {
  let rodadas = 0;
  let solta;
  const acionar = coalescer(async () => {
    rodadas += 1;
    if (rodadas === 1) await new Promise((r) => (solta = r));
  });
  const primeira = acionar();
  await acionar();
  await acionar();
  await acionar();
  assert.equal(rodadas, 1);
  solta();
  await primeira;
  assert.equal(rodadas, 2);
  await acionar();
  assert.equal(rodadas, 3);
});

test("coalescer: sem pedido durante a rodada não há rodada extra", async () => {
  let rodadas = 0;
  const acionar = coalescer(async () => {
    rodadas += 1;
  });
  await acionar();
  assert.equal(rodadas, 1);
});

test("marcaSemValor distingue sem fonte de sem cotação", async () => {
  const { marcaSemValor } = await import("../../renderer/js/telas/_tabela.js");
  assert.equal(marcaSemValor("sem_fonte"), "sem fonte de preço");
  assert.equal(marcaSemValor("sem_dados"), "sem cotação");
});
