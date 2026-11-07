// As peças reais da atualização (net.fetch e o autoUpdater do electron-updater) e o feed de teste.
// Não faz require("electron"): o main.js passa `app` e `net`. O electron-updater só é carregado
// depois do clique em "Baixar e instalar", no NSIS e no AppImage.
const fs = require("node:fs");
const path = require("node:path");
const { formatoInstalado, criarAtualizacao } = require("./atualizacao.js");

const URL_ULTIMA = "https://api.github.com/repos/guilhermebecko/Guru-de-Investimentos/releases/latest";
const PRAZO_MS = 30000;
const LIMITE_BYTES = 1024 * 1024;
const FORMATOS_TESTE = new Set(["nsis", "appimage", "dmg", "deb"]);

function falhou(causa) {
  const e = new Error("Não foi possível procurar atualizações. Tente de novo mais tarde.", causa ? { cause: causa } : undefined);
  e.code = "ATUALIZACAO_FALHOU";
  return e;
}

// Lê o corpo até LIMITE_BYTES; acima disso é erro.
async function lerCorpo(resposta) {
  const leitor = resposta.body.getReader();
  const partes = [];
  let total = 0;
  for (;;) {
    const { done, value } = await leitor.read();
    if (done) break;
    total += value.byteLength;
    if (total > LIMITE_BYTES) {
      await leitor.cancel().catch(() => {});
      throw falhou();
    }
    partes.push(value);
  }
  return Buffer.concat(partes).toString("utf8");
}

// GET releases/latest (ignora rascunhos e pré-lançamentos). 404: nenhuma release publicada, null.
function criarBuscarUltima({ net, versaoApp }) {
  return async function buscarUltima() {
    try {
      const resposta = await net.fetch(URL_ULTIMA, {
        headers: { Accept: "application/vnd.github+json", "User-Agent": `GuruDeInvestimentos/${versaoApp}` },
        signal: AbortSignal.timeout(PRAZO_MS),
      });
      if (resposta.status === 404) {
        await resposta.body?.cancel().catch(() => {});
        return null;
      }
      if (!resposta.ok) throw falhou();
      return JSON.parse(await lerCorpo(resposta));
    } catch (e) {
      // A mensagem da rede pode trazer detalhes do sistema: só o code atravessa.
      throw e && e.code === "ATUALIZACAO_FALHOU" ? e : falhou(e);
    }
  };
}

// O instalador de verdade, só no NSIS e no AppImage. Nada sozinho: sem download automático, sem
// instalar ao sair, sem pré-lançamento nem versão menor, e sem log (ele gravaria caminhos).
function criarInstaladorReal() {
  const { autoUpdater } = require("electron-updater");
  autoUpdater.autoDownload = false;
  autoUpdater.autoInstallOnAppQuit = false;
  autoUpdater.allowPrerelease = false;
  autoUpdater.allowDowngrade = false;
  autoUpdater.logger = null;
  // As falhas chegam pelas promessas; sem ouvinte, o evento "error" derrubaria o processo.
  autoUpdater.on("error", () => {});
  return {
    procurar: async () => {
      const r = await autoUpdater.checkForUpdates();
      return r && r.updateInfo ? r.updateInfo.version : null;
    },
    baixar: async (aoProgresso) => {
      const ouvir = (p) => aoProgresso(p && p.percent);
      autoUpdater.on("download-progress", ouvir);
      try {
        await autoUpdater.downloadUpdate();
      } finally {
        autoUpdater.removeListener("download-progress", ouvir);
      }
    },
    instalar: () => autoUpdater.quitAndInstall(false, true),
  };
}

const esperar = (ms) => new Promise((r) => setTimeout(r, ms));

// GURU_TESTE_ATUALIZACAO: { formato, ultima: { tag, html_url } | null, erro }. A procura lê o
// arquivo no lugar da API; o instalador falso avança 0, 50 e 100 e não fecha o app.
function criarFeedTeste(arquivo, { intervaloMs = 150 } = {}) {
  const dados = JSON.parse(fs.readFileSync(arquivo, "utf8"));
  if (!FORMATOS_TESTE.has(dados.formato)) throw new Error(`formato do feed de teste inválido: ${dados.formato}`);
  const ultima = dados.ultima ? { tag_name: dados.ultima.tag, html_url: dados.ultima.html_url } : null;
  return {
    formato: dados.formato,
    buscarUltima: async () => {
      await esperar(intervaloMs);
      if (dados.erro) throw falhou();
      return ultima;
    },
    criarInstalador: () => ({
      procurar: async () => (ultima ? String(ultima.tag_name).replace(/^v/, "") : null),
      baixar: async (aoProgresso) => {
        for (const p of [0, 50, 100]) {
          aoProgresso(p);
          await esperar(intervaloMs);
        }
      },
      instalar: () => {},
    }),
  };
}

// Monta a atualização do app. Fora do pacote, só o feed de teste consulta alguma coisa.
function montarAtualizacao({ app, net, env = process.env, plataforma = process.platform }) {
  const versaoAtual = app.getVersion();
  const arquivoConfig = path.join(app.getPath("userData"), "atualizacao.json");
  const teste = !app.isPackaged ? env.GURU_TESTE_ATUALIZACAO : undefined;
  if (teste) {
    const feed = criarFeedTeste(teste);
    return criarAtualizacao({
      versaoAtual,
      formato: feed.formato,
      arquivoConfig,
      buscarUltima: feed.buscarUltima,
      criarInstalador: feed.criarInstalador,
    });
  }
  return criarAtualizacao({
    versaoAtual,
    formato: formatoInstalado({ empacotado: app.isPackaged, plataforma, env, execPath: process.execPath }),
    arquivoConfig,
    buscarUltima: criarBuscarUltima({ net, versaoApp: versaoAtual }),
    criarInstalador: criarInstaladorReal,
  });
}

module.exports = { criarBuscarUltima, criarInstaladorReal, criarFeedTeste, montarAtualizacao };
