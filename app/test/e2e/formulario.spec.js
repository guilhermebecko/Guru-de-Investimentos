const { test, expect } = require("@playwright/test");
const { lancar } = require("./_app");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

// Datas futuras derivadas do ano atual, para os testes não vencerem com o tempo.
const ANO = new Date().getFullYear();
const VENC_CDB = `${ANO + 3}-03-15`;
const VENC_CDB_BR = `15/03/${ANO + 3}`;
const VENC_SELIC = `${ANO + 4}-03-01`;
const PRAZO = `${ANO + 4}-12`;


async function abrirNovo(page) {
  await page.evaluate(() => {
    window.location.hash = "#/investimentos/novo";
  });
  await expect(page.getByRole("heading", { name: "Novo investimento" })).toBeVisible();
}

async function preencherCdb(page) {
  await page.getByLabel("Emissor").fill("Banco Inter");
  await page.getByLabel("Rendimento", { exact: true }).fill("102");
  await page.getByLabel("Data da aplicação").fill("2025-03-15");
  await page.getByLabel("Vencimento").fill(VENC_CDB);
  await page.getByLabel("Valor aplicado").fill("20.000,00");
}

async function salvar(page) {
  await page.getByRole("button", { name: "Salvar", exact: true }).click();
}

test("lança um CDB e ele aparece na lista", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await preencherCdb(page);
    await expect(page.locator("#f-taxa-dica")).toHaveText("Lê-se: 102% do CDI");
    await salvar(page);
    const linha = page.locator("main tbody tr").first();
    await expect(linha).toContainText("CDB Banco Inter");
    await expect(linha).toContainText("102% do CDI");
    await expect(linha).toContainText("R$ 20.000,00");
  } finally {
    await app.close();
  }
});

test("LCI preenche IR isento e FGC", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("rf-lci");
    const ir = page.getByLabel("Imposto de renda");
    await expect(ir).toHaveValue("isento");
    await expect(ir).toBeDisabled();
    await expect(page.getByLabel("Garantia")).toHaveText("FGC");
  } finally {
    await app.close();
  }
});

test("Tesouro IPCA+ força o indexador", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("td-ipca");
    const idx = page.getByLabel("Indexador");
    await expect(idx).toBeDisabled();
    await expect(idx.locator("option:checked")).toHaveText("Híbrido (IPCA + taxa)");
    await expect(page.getByLabel("Benchmark")).toBeHidden();
    const liq = page.getByLabel("Liquidez");
    await expect(liq).toBeDisabled();
    await expect(liq.locator("option:checked")).toHaveText("D+1");
    await expect(page.getByLabel("Emissor")).toBeHidden();
  } finally {
    await app.close();
  }
});

test("ação troca para ticker, quantidade e preço médio", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("rv-acao");
    await expect(page.getByLabel("Indexador")).toBeHidden();
    await page.getByLabel("Ticker").fill("itub4");
    await page.getByLabel("Quantidade").fill("150");
    await page.getByLabel("Preço médio").fill("31,42");
    await page.getByLabel("Data da primeira compra").fill("2025-03-15");
    await salvar(page);
    const linha = page.locator("main tbody tr").first();
    await expect(linha).toContainText("ITUB4");
    await expect(linha).toContainText("R$ 4.713,00");
  } finally {
    await app.close();
  }
});

test("trocar CDB → Ação → LCI não grava campo fantasma", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("rv-acao");
    await page.getByLabel("Ticker").fill("ITUB4");
    await page.getByLabel("Tipo").selectOption("rf-lci");
    await preencherCdb(page);
    await salvar(page);
    await expect(page.locator("main tbody tr").first()).toContainText("LCI Banco Inter");
    await page.getByRole("link", { name: "Editar" }).click();
    await expect(page.getByRole("heading", { name: "Editar investimento" })).toBeVisible();
    await expect(page.getByLabel("Ticker")).toBeHidden();
    await expect(page.getByLabel("Imposto de renda")).toHaveValue("isento");
    const inv = await page.evaluate(() =>
      import("./js/api.js").then(({ api }) => api.investimentos.listar()),
    );
    expect(inv[0].ticker ?? null).toBeNull();
  } finally {
    await app.close();
  }
});

test("erros aparecem no campo", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await salvar(page);
    const emissor = page.getByLabel("Emissor");
    await expect(emissor).toHaveAttribute("aria-invalid", "true");
    await expect(page.locator("#e-emissor")).toHaveText("Informe o emissor.");
    await expect(emissor).toHaveAttribute("aria-describedby", /e-emissor/);
    await expect(emissor).toBeFocused();
    await expect(page.locator("#e-valorAplicadoCentavos")).toHaveText("Informe o valor aplicado.");
    await expect(page.getByLabel("Valor aplicado")).toHaveAttribute("aria-invalid", "true");

    await page.evaluate(() => {
      window.__chamadas = 0;
      import("./js/api.js").then(({ api }) => {
        const criar = api.investimentos.criar;
        api.investimentos.criar = (d) => {
          window.__chamadas++;
          return criar(d);
        };
      });
    });
    await page.getByLabel("Valor aplicado").fill("1,234,56");
    await salvar(page);
    await expect(page.locator("#e-valorAplicadoCentavos")).toHaveText(
      "Informe um valor em reais, por exemplo 1.234,56.",
    );
    expect(await page.evaluate(() => window.__chamadas)).toBe(0);
  } finally {
    await app.close();
  }
});

test("taxa com casas decimais volta igual", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("td-selic");
    await page.getByLabel("Taxa acima da Selic").fill("0,0905");
    await expect(page.locator("#f-taxa-dica")).toHaveText("Lê-se: Selic + 0,0905% ao ano");
    await page.getByLabel("Data da aplicação").fill("2025-03-15");
    await page.getByLabel("Vencimento").fill(VENC_SELIC);
    await page.getByLabel("Valor aplicado").fill("1000");
    await salvar(page);
    await expect(page.locator("main tbody tr").first()).toContainText("Selic + 0,0905% ao ano");
  } finally {
    await app.close();
  }
});

test("taxa com ponto decimal é aceita e inválida não envia", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Rendimento", { exact: true }).fill("102.5");
    await expect(page.locator("#f-taxa-dica")).toHaveText("Lê-se: 102,5% do CDI");
    await page.getByLabel("Rendimento", { exact: true }).fill("1.000,5");
    await salvar(page);
    await expect(page.locator("#e-taxa")).toHaveText("Informe uma taxa válida.");
  } finally {
    await app.close();
  }
});

test("edita e mantém o id", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await preencherCdb(page);
    await salvar(page);
    await page.getByRole("link", { name: "Editar" }).click();
    await expect(page.getByLabel("Rendimento", { exact: true })).toHaveValue("102");
    await expect(page.getByLabel("Valor aplicado")).toHaveValue("20.000,00");
    await page.getByLabel("Rendimento", { exact: true }).fill("105");
    await salvar(page);
    await expect(page.locator("main tbody tr")).toHaveCount(1);
    await expect(page.locator("main tbody tr").first()).toContainText("105% do CDI");
  } finally {
    await app.close();
  }
});

test("editar id que não existe", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await page.evaluate(() => {
      window.location.hash = "#/investimentos/999";
    });
    await expect(page.locator("main")).toContainText("Este investimento não existe mais. Atualize a lista.");
    await expect(page.getByRole("link", { name: "Voltar para a lista" })).toHaveAttribute(
      "href",
      "#/investimentos",
    );
  } finally {
    await app.close();
  }
});

test("salvar e adicionar outro mantém tipo e instituição", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("rf-lca");
    await page.getByLabel("Corretora ou banco").selectOption("XP Investimentos");
    await preencherCdb(page);
    await page.getByRole("button", { name: "Salvar e adicionar outro" }).click();
    await expect(page.getByText("Investimento salvo.")).toBeVisible();
    await expect(page.getByLabel("Tipo")).toHaveValue("rf-lca");
    await expect(page.getByLabel("Corretora ou banco")).toHaveValue("XP Investimentos");
    await expect(page.getByLabel("Emissor")).toHaveValue("");
  } finally {
    await app.close();
  }
});

test("os dados continuam depois de reabrir o app", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "guru-e2e-"));
  let app = await lancar(dir);
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await preencherCdb(page);
    await salvar(page);
    await expect(page.locator("main tbody tr")).toHaveCount(1);
  } finally {
    await app.close();
  }
  app = await lancar(dir);
  try {
    const page = await app.firstWindow();
    await page.getByRole("link", { name: "Investimentos", exact: true }).click();
    await expect(page.locator("main tbody tr").first()).toContainText("CDB Banco Inter");
  } finally {
    await app.close();
  }
});

test("sair do Tesouro volta indexador e liquidez ao padrão e os libera", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("td-ipca");
    await page.getByLabel("Tipo").selectOption("rf-cdb");
    const idx = page.getByLabel("Indexador");
    const liq = page.getByLabel("Liquidez");
    await expect(idx).toBeEnabled();
    await expect(liq).toBeEnabled();
    await expect(idx.locator("option:checked")).toHaveText("Pós-fixado");
    await expect(liq.locator("option:checked")).toHaveText("No vencimento");
    await expect(page.getByLabel("Nome da instituição")).toBeHidden();
  } finally {
    await app.close();
  }
});

test("Criptoativo deixa a liquidez em Diária e salva sem vencimento", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("cripto");
    await expect(page.getByLabel("Liquidez").locator("option:checked")).toHaveText("Diária");
    await page.getByLabel("Nome", { exact: true }).fill("Bitcoin");
    await page.getByLabel("Data da aplicação").fill("2025-03-15");
    await page.getByLabel("Valor aplicado").fill("1.000,00");
    await salvar(page);
    const linha = page.locator("main tbody tr").first();
    await expect(linha).toContainText("Bitcoin");
    await expect(linha).toContainText("R$ 1.000,00");
  } finally {
    await app.close();
  }
});

test("salvar e adicionar outro com Ação deixa o foco no ticker", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("rv-acao");
    await page.getByLabel("Ticker").fill("B3SA3");
    await page.getByLabel("Quantidade").fill("10");
    await page.getByLabel("Preço médio").fill("12,50");
    await page.getByLabel("Data da primeira compra").fill("2025-03-15");
    await page.getByRole("button", { name: "Salvar e adicionar outro" }).click();
    await expect(page.getByText("Investimento salvo.")).toBeVisible();
    await expect(page.getByLabel("Ticker")).toBeFocused();
  } finally {
    await app.close();
  }
});

test("Vincular a uma meta começa em Nenhuma, lista as metas e não grava metaId sem escolha", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await page.evaluate(
      (prazo) =>
        import("./js/api.js").then(({ api }) => api.metas.criar({ nome: "Reserva", valorAlvoCentavos: 3000000, prazo })),
      `${ANO + 5}-01`,
    );
    await abrirNovo(page);
    const campo = page.getByLabel("Vincular a uma meta");
    await expect(campo.locator("option")).toHaveText(["Nenhuma", "Reserva"]);
    await expect(campo).toHaveValue("");
    await preencherCdb(page);
    await salvar(page);
    await expect(page.locator("main tbody tr").first()).toContainText("CDB Banco Inter");
    const inv = await page.evaluate(() => import("./js/api.js").then(({ api }) => api.investimentos.listar()));
    expect(inv[0].metaId ?? null).toBeNull();
  } finally {
    await app.close();
  }
});

test("fundo guarda o CNPJ e a edição o mostra formatado", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    await abrirNovo(page);
    await page.getByLabel("Tipo").selectOption("fundo");
    await page.getByLabel("Nome", { exact: true }).fill("Fundo Teste");
    await page.getByLabel("CNPJ do fundo (opcional)").fill("123");
    await page.getByLabel("Data da aplicação").fill("2025-03-15");
    await page.getByLabel("Valor aplicado").fill("10.000,00");
    await salvar(page);
    await expect(page.locator("#e-cnpj")).toHaveText("CNPJ inválido. Confira os 14 dígitos.");
    await page.getByLabel("CNPJ do fundo (opcional)").fill("11222333000181");
    await salvar(page);
    await expect(page.locator("main tbody tr").first()).toContainText("Fundo Teste");
    await page.getByRole("link", { name: "Editar" }).click();
    await expect(page.getByRole("heading", { name: "Editar investimento" })).toBeVisible();
    await expect(page.getByLabel("CNPJ do fundo (opcional)")).toHaveValue("11.222.333/0001-81");
  } finally {
    await app.close();
  }
});
