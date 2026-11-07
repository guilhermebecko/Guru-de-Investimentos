const { test, expect } = require("@playwright/test");
const { lancar } = require("./_app");

const main = (page) => page.locator("main");

const CARTOES = [
  ["Não é recomendação", "RECOMENDACAO"],
  ["Inteligência artificial", "IA"],
  ["Dados de mercado", "MERCADO"],
  ["Privacidade", "PRIVACIDADE"],
  ["Licença e garantia", "LICENCA"],
];

test("Sobre e avisos: versão, os seis textos de AVISOS e os avisos de terceiros só no clique", async () => {
  const app = await lancar();
  try {
    const page = await app.firstWindow();
    const erros = [];
    page.on("console", (m) => m.type() === "error" && erros.push(m.text()));
    page.on("pageerror", (e) => erros.push(String(e)));

    const nav = page.locator("nav a");
    await expect(nav.last()).toHaveText("Sobre e avisos");
    await nav.last().click();
    await expect(main(page).getByRole("heading", { name: "Sobre e avisos", level: 1 })).toBeVisible();
    await expect(page.locator("nav a.on")).toHaveAttribute("href", "#/sobre");

    const info = await page.evaluate(() => import("./js/api.js").then(({ api }) => api.app.info()));
    await expect(main(page)).toContainText(`Versão ${info.versaoApp} · Núcleo ${info.coreVersion}`);
    expect(info.versaoApp).toBe("0.9.0");
    await expect(main(page).getByRole("link", { name: /github\.com\/guilhermebecko\/Guru-de-Investimentos/ })).toHaveAttribute(
      "href",
      "https://github.com/guilhermebecko/Guru-de-Investimentos",
    );
    await expect(main(page)).toContainText("Licença MIT");
    await expect(main(page).getByTestId("sobre-atualizacoes")).toHaveCount(1);

    const AVISOS = await page.evaluate(() => import("./js/avisos.js").then((m) => ({ ...m.AVISOS })));
    for (const [titulo, chave] of CARTOES) {
      const cartao = main(page).getByRole("region", { name: titulo, exact: true });
      await expect(cartao.getByRole("heading", { name: titulo, exact: true })).toBeVisible();
      await expect(cartao.locator("p")).toHaveText(AVISOS[chave]);
    }
    await expect(main(page).locator(".faixa")).toHaveText(AVISOS.REVISAO_JURIDICA);

    // Nada é carregado antes do clique.
    const terceiros = main(page).getByRole("region", { name: "Avisos de terceiros", exact: true });
    await expect(terceiros.locator("pre")).toHaveCount(0);
    await terceiros.getByRole("button", { name: "Ver avisos de terceiros" }).click();
    const pre = terceiros.locator("pre");
    await expect(pre).toContainText("== serde ");
    await expect(pre).toContainText("== electron-updater 6.8.10 (MIT) ==");
    await expect(pre).toContainText("SIL OPEN FONT LICENSE");
    await expect(pre).toContainText("O Electron e o Chromium trazem as próprias licenças");

    expect(erros).toEqual([]);
  } finally {
    await app.close();
  }
});
