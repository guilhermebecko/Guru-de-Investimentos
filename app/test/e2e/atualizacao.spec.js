// Atualização com o feed falso (GURU_TESTE_ATUALIZACAO): nenhuma chamada à rede. O arquivo diz o
// formato instalado e a última release; o instalador falso avança 0, 50 e 100 e não fecha o app.
const { test, expect } = require("@playwright/test");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { lancar } = require("./_app");

const FIXTURES = path.join(__dirname, "fixtures", "atualizacao");
const feed = (nome) => ({ GURU_TESTE_ATUALIZACAO: path.join(FIXTURES, nome) });
const novaPasta = () => fs.mkdtempSync(path.join(os.tmpdir(), "guru-e2e-"));

function vigiarErros(page) {
  const erros = [];
  page.on("console", (m) => m.type() === "error" && erros.push(m.text()));
  page.on("pageerror", (e) => erros.push(String(e)));
  return erros;
}

async function abrirSobre(page) {
  await page.locator("nav a", { hasText: "Sobre e avisos" }).click();
  await expect(page.locator("main").getByRole("heading", { name: "Sobre e avisos", level: 1 })).toBeVisible();
  return page.locator("main").getByRole("region", { name: "Atualizações", exact: true });
}

test("nova versão no NSIS: a faixa oferece baixar, chega a baixada e reiniciar não fecha no teste", async () => {
  const app = await lancar(undefined, feed("nova-nsis.json"));
  try {
    const page = await app.firstWindow();
    const erros = vigiarErros(page);
    const faixa = page.getByTestId("atualizacao");
    await expect(faixa).toBeVisible();
    await expect(faixa).toContainText("Nova versão disponível: 9.9.0.");
    await expect(faixa.getByRole("link", { name: "Ver no GitHub" })).toHaveCount(0);
    await faixa.getByRole("button", { name: "Baixar e instalar" }).click();
    await expect(faixa).toContainText("Versão 9.9.0 baixada. Reinicie para atualizar.");
    await faixa.getByRole("button", { name: "Reiniciar e atualizar" }).click();
    // O instalador falso só marca `reiniciando`: o app continua aberto e respondendo.
    await expect
      .poll(() => page.evaluate(() => import("./js/api.js").then(({ api }) => api.atualizacao.estado())))
      .toMatchObject({ estado: "baixada", reiniciando: true, versaoNova: "9.9.0" });
    await expect(faixa).toContainText("Versão 9.9.0 baixada.");
    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});

test("nova versão no .deb: só o link da página, sem baixar", async () => {
  const app = await lancar(undefined, feed("nova-deb.json"));
  try {
    const page = await app.firstWindow();
    const erros = vigiarErros(page);
    const faixa = page.getByTestId("atualizacao");
    await expect(faixa).toContainText("Nova versão disponível: 9.9.0.");
    await expect(faixa.getByRole("link", { name: "Ver no GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/guilhermebecko/Guru-de-Investimentos/releases/tag/v9.9.0",
    );
    await expect(page.getByRole("button", { name: "Baixar e instalar" })).toHaveCount(0);
    const secao = await abrirSobre(page);
    await expect(secao).toContainText("Nova versão disponível: 9.9.0.");
    await expect(secao.getByRole("link", { name: "Ver no GitHub" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Baixar e instalar" })).toHaveCount(0);
    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});

test("mesma versão: sem faixa, e Sobre diz que está na mais recente", async () => {
  const app = await lancar(undefined, feed("igual.json"));
  try {
    const page = await app.firstWindow();
    const erros = vigiarErros(page);
    const secao = await abrirSobre(page);
    await expect(secao.getByRole("status")).toHaveText("Você está na versão mais recente (0.9.0).");
    await expect(page.getByTestId("atualizacao")).toBeHidden();
    await expect(secao.getByRole("switch", { name: "Procurar atualizações ao abrir o app" })).toBeChecked();
    await expect(secao).toContainText("Consulta a página de versões do projeto no GitHub. Nada é baixado sem o seu clique.");
    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});

test("falha na procura: sem faixa, e Procurar agora mostra o erro", async () => {
  const app = await lancar(undefined, feed("erro.json"));
  try {
    const page = await app.firstWindow();
    const erros = vigiarErros(page);
    const secao = await abrirSobre(page);
    await secao.getByRole("button", { name: "Procurar agora" }).click();
    await expect(secao.getByRole("alert")).toHaveText("Não foi possível procurar atualizações. Tente de novo mais tarde.");
    await expect(page.getByTestId("atualizacao")).toBeHidden();
    await expect(secao.getByRole("button", { name: "Procurar agora" })).toBeEnabled();
    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});

test("o ajuste desligado vale na próxima abertura, e Procurar agora ainda procura", async () => {
  const dir = novaPasta();
  let app = await lancar(dir, feed("nova-nsis.json"));
  try {
    const page = await app.firstWindow();
    await expect(page.getByTestId("atualizacao")).toBeVisible();
    const secao = await abrirSobre(page);
    const ajuste = secao.getByRole("switch", { name: "Procurar atualizações ao abrir o app" });
    await ajuste.uncheck();
    await expect
      .poll(() => page.evaluate(() => import("./js/api.js").then(({ api }) => api.atualizacao.estado())))
      .toMatchObject({ procurarAoAbrir: false });
  } finally {
    await app.close();
  }
  app = await lancar(dir, feed("nova-nsis.json"));
  try {
    const page = await app.firstWindow();
    const erros = vigiarErros(page);
    const secao = await abrirSobre(page);
    await expect(secao.getByRole("switch", { name: "Procurar atualizações ao abrir o app" })).not.toBeChecked();
    expect(await page.evaluate(() => import("./js/api.js").then(({ api }) => api.atualizacao.estado()))).toMatchObject({
      estado: "parado",
      versaoNova: null,
    });
    await expect(page.getByTestId("atualizacao")).toBeHidden();
    await secao.getByRole("button", { name: "Procurar agora" }).click();
    await expect(secao.getByRole("status")).toHaveText("Nova versão disponível: 9.9.0.");
    await expect(page.getByTestId("atualizacao")).toContainText("Nova versão disponível: 9.9.0.");
    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});

test("sem o feed de teste, fora do pacote: nada é consultado e não há Procurar agora", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    const erros = vigiarErros(page);
    const secao = await abrirSobre(page);
    await expect(secao.getByRole("status")).toHaveText("A procura de atualizações só funciona no app instalado.");
    await expect(secao.getByRole("button", { name: "Procurar agora" })).toHaveCount(0);
    await expect(page.getByTestId("atualizacao")).toBeHidden();
    expect(await page.evaluate(() => import("./js/api.js").then(({ api }) => api.atualizacao.estado()))).toMatchObject({
      estado: "indisponivel",
      formato: "dev",
    });
    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});
