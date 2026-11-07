// Atualização do app: a lógica pura, sem require("electron"). A procura (buscarUltima) e o
// instalador (criarInstalador) são injetados; ver main/atualizacao-electron.js para as peças reais
// e para o feed de teste.
//
// Regras: procurar nunca baixa; só NSIS e AppImage instalam, e só depois do clique em "Baixar e
// instalar"; só "Reiniciar e atualizar" chama instalar(). Fora do app instalado, nada é consultado.
const fs = require("node:fs");
const path = require("node:path");

const RELEASES = "https://github.com/guilhermebecko/Guru-de-Investimentos/releases/";
const PAGINA_PADRAO = `${RELEASES}latest`;
const TAG = /^v(\d+)\.(\d+)\.(\d+)$/;
const INSTALAVEIS = new Set(["nsis", "appimage"]);

const MENSAGENS = {
  ATUALIZACAO_INDISPONIVEL: "A procura de atualizações só funciona no app instalado.",
  ATUALIZACAO_NAO_INSTALAVEL: "Neste sistema o app não se atualiza sozinho: baixe o instalador novo na página da versão.",
  ATUALIZACAO_FALHOU: "Não foi possível procurar atualizações. Tente de novo mais tarde.",
  ATUALIZACAO_CONFIG_INVALIDA: "O ajuste de atualização não é válido.",
  ATUALIZACAO_CONFIG_FALHOU: "Não foi possível salvar o ajuste de atualização.",
};

function erroAtualizacao(code, causa) {
  const e = new Error(MENSAGENS[code], causa ? { cause: causa } : undefined);
  e.code = code;
  return e;
}

// -1, 0 ou 1, comparando "X.Y.Z" número a número ("0.9.0" < "0.10.0").
function compararVersoes(a, b) {
  const pa = String(a).split(".").map(Number);
  const pb = String(b).split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] || 0;
    const y = pb[i] || 0;
    if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

// Lê o JSON de releases/latest: a versão vem só de uma tag vX.Y.Z, e a página só do repositório.
// `versaoAtual` não muda a leitura; quem decide se é nova é criarAtualizacao.
function lerRelease(json, _versaoAtual) {
  const tag = json && typeof json.tag_name === "string" ? json.tag_name : "";
  const m = TAG.exec(tag);
  if (!m) throw erroAtualizacao("ATUALIZACAO_FALHOU");
  const versao = `${Number(m[1])}.${Number(m[2])}.${Number(m[3])}`;
  const url = json.html_url;
  const pagina = typeof url === "string" && url.startsWith(RELEASES) ? url : PAGINA_PADRAO;
  return { versao, pagina };
}

// Como o app foi instalado. Linux fora do AppImage é tratado como .deb.
function formatoInstalado({ empacotado, plataforma, env, execPath = "" }) {
  if (!empacotado) return "dev";
  if (plataforma === "win32") return "nsis";
  if (plataforma === "darwin") return "dmg";
  // APPIMAGE pode vir herdado de outro AppImage que abriu o app: só vale se este processo roda de
  // dentro da pasta montada do AppImage (APPDIR), senão a atualização sobrescreveria outro arquivo.
  const appimage = env && env.APPIMAGE && env.APPDIR && execPath.startsWith(env.APPDIR);
  return appimage ? "appimage" : "deb";
}

// Sem arquivo, ilegível ou com outro tipo: ligado (o próximo salvarConfig regrava o arquivo).
function lerConfig(arquivo) {
  try {
    const dados = JSON.parse(fs.readFileSync(arquivo, "utf8"));
    if (dados && typeof dados.procurarAoAbrir === "boolean") return dados.procurarAoAbrir;
  } catch {
    // Sem arquivo ou ilegível: fica o padrão.
  }
  return true;
}

function criarAtualizacao({ versaoAtual, formato, arquivoConfig, buscarUltima, criarInstalador }) {
  const instalavel = INSTALAVEIS.has(formato);
  let procurarAoAbrir = lerConfig(arquivoConfig);
  // O ajuste vale a partir da próxima abertura: a procura de abertura usa o valor lido agora.
  const procurarNestaAbertura = procurarAoAbrir && formato !== "dev";
  let abriu = false;
  let estado = formato === "dev" ? "indisponivel" : "parado";
  let versaoNova = null;
  let pagina = null;
  let progresso = null;
  let erro = null;
  let reiniciando = false;
  let instalador = null;

  function foto() {
    // Até a procura de abertura começar, o estado já diz que ela vem: a tela que ler antes do
    // did-finish-load acompanha em vez de parar em "parado".
    const visto = estado === "parado" && procurarNestaAbertura && !abriu ? "procurando" : estado;
    return {
      estado: visto,
      versaoAtual,
      versaoNova,
      pagina,
      instalavel,
      formato,
      procurarAoAbrir,
      progresso,
      erro: erro ? { ...erro } : null,
      reiniciando,
    };
  }

  // Marca a falha no estado. Na procura, o estado vira "erro"; no download, volta a "disponivel"
  // com o erro, para a faixa continuar com a versão nova e o botão de tentar de novo.
  function falhar(e, voltarPara = "erro") {
    const code = e && e.code && MENSAGENS[e.code] ? e.code : "ATUALIZACAO_FALHOU";
    estado = voltarPara;
    progresso = null;
    erro = { code, message: MENSAGENS[code] };
    return code === e?.code ? e : erroAtualizacao(code, e);
  }

  async function procurar() {
    if (formato === "dev") throw erroAtualizacao("ATUALIZACAO_INDISPONIVEL");
    // Em curso, ou já baixada (procurar de novo jogaria fora o download): devolve o estado.
    if (estado === "procurando" || estado === "baixando" || estado === "baixada") return foto();
    abriu = true;
    estado = "procurando";
    erro = null;
    progresso = null;
    try {
      const json = await buscarUltima();
      // null: o repositório ainda não tem release publicada (a API responde 404).
      const release = json === null ? null : lerRelease(json, versaoAtual);
      if (release && compararVersoes(release.versao, versaoAtual) > 0) {
        estado = "disponivel";
        versaoNova = release.versao;
        pagina = release.pagina;
      } else {
        estado = "atualizado";
        versaoNova = null;
        pagina = null;
      }
      return foto();
    } catch (e) {
      throw falhar(e);
    }
  }

  async function aoAbrir() {
    if (abriu || !procurarNestaAbertura) {
      abriu = true;
      return foto();
    }
    abriu = true;
    try {
      await procurar();
    } catch {
      // A falha da procura de abertura fica no estado "erro", sem faixa e sem lançar.
    }
    return foto();
  }

  async function salvarConfig(dados) {
    const valor = dados && dados.procurarAoAbrir;
    if (typeof valor !== "boolean") throw erroAtualizacao("ATUALIZACAO_CONFIG_INVALIDA");
    const temporario = `${arquivoConfig}.tmp`;
    try {
      await fs.promises.mkdir(path.dirname(arquivoConfig), { recursive: true });
      await fs.promises.writeFile(temporario, `${JSON.stringify({ procurarAoAbrir: valor })}\n`);
      await fs.promises.rename(temporario, arquivoConfig);
    } catch (e) {
      // O erro do fs traz o caminho na mensagem: não atravessa a ponte.
      throw erroAtualizacao("ATUALIZACAO_CONFIG_FALHOU", e);
    }
    procurarAoAbrir = valor;
    return foto();
  }

  async function baixar() {
    if (formato === "dev") throw erroAtualizacao("ATUALIZACAO_INDISPONIVEL");
    if (!instalavel) throw erroAtualizacao("ATUALIZACAO_NAO_INSTALAVEL");
    if (estado === "baixando" || estado === "baixada") return foto();
    if (estado !== "disponivel") throw erroAtualizacao("ATUALIZACAO_FALHOU");
    const esperada = versaoNova;
    estado = "baixando";
    progresso = 0;
    erro = null;
    try {
      instalador ??= criarInstalador();
      // O latest.yml precisa anunciar a mesma versão que a API do GitHub.
      const versaoDoYml = await instalador.procurar();
      if (versaoDoYml !== esperada) throw erroAtualizacao("ATUALIZACAO_FALHOU");
      await instalador.baixar((p) => {
        const n = Math.round(Number(p));
        if (Number.isFinite(n)) progresso = Math.min(100, Math.max(0, n));
      });
      estado = "baixada";
      progresso = 100;
      return foto();
    } catch (e) {
      throw falhar(e, "disponivel");
    }
  }

  function reiniciar() {
    if (estado !== "baixada") throw erroAtualizacao("ATUALIZACAO_FALHOU");
    // No app instalado, quitAndInstall(false, true) fecha e reabre; o falso só marca reiniciando.
    instalador.instalar();
    reiniciando = true;
    return foto();
  }

  return { estado: foto, procurar, salvarConfig, baixar, reiniciar, aoAbrir };
}

module.exports = { compararVersoes, lerRelease, formatoInstalado, criarAtualizacao };
