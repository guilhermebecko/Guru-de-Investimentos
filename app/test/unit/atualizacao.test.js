const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { compararVersoes, lerRelease, formatoInstalado, criarAtualizacao } = require("../../main/atualizacao.js");
const { criarBuscarUltima, criarFeedTeste } = require("../../main/atualizacao-electron.js");

const PAGINA_PADRAO = "https://github.com/guilhermebecko/Guru-de-Investimentos/releases/latest";
const PAGINA_091 = "https://github.com/guilhermebecko/Guru-de-Investimentos/releases/tag/v0.9.1";

function pastaTemp() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "guru-atualizacao-"));
}

// Monta a atualização com fakes que contam chamadas. `release` é o JSON que buscarUltima devolve
// (ou uma função que lança); `versaoDoYml` é o que o instalador lê do latest.yml.
function montar({ formato = "nsis", release, versaoDoYml = "0.9.1", config, progressos = [0, 50, 100] } = {}) {
  const dir = pastaTemp();
  const arquivoConfig = path.join(dir, "atualizacao.json");
  if (config !== undefined) fs.writeFileSync(arquivoConfig, config);
  const contagem = { buscarUltima: 0, criarInstalador: 0, procurar: 0, baixar: 0, instalar: 0 };
  const vistos = [];
  let atualizacao;
  const instalador = {
    procurar: async () => {
      contagem.procurar++;
      return versaoDoYml;
    },
    baixar: async (aoProgresso) => {
      contagem.baixar++;
      for (const p of progressos) {
        aoProgresso(p);
        vistos.push(atualizacao.estado());
      }
    },
    instalar: () => {
      contagem.instalar++;
    },
  };
  atualizacao = criarAtualizacao({
    versaoAtual: "0.9.0",
    formato,
    arquivoConfig,
    buscarUltima: async () => {
      contagem.buscarUltima++;
      if (typeof release === "function") return release();
      return release === undefined ? { tag_name: "v0.9.1", html_url: PAGINA_091 } : release;
    },
    criarInstalador: () => {
      contagem.criarInstalador++;
      return instalador;
    },
  });
  return { atualizacao, contagem, vistos, arquivoConfig };
}

const comCode = (code) => (e) => e instanceof Error && e.code === code && typeof e.message === "string";

test("compara_versoes", () => {
  assert.equal(compararVersoes("0.9.0", "0.10.0"), -1);
  assert.equal(compararVersoes("0.10.0", "0.9.0"), 1);
  assert.equal(compararVersoes("1.0.0", "0.99.99"), 1);
  assert.equal(compararVersoes("0.9.0", "0.9.0"), 0);
  assert.equal(compararVersoes("0.9.1", "0.9.0"), 1);
});

test("procurar_nao_baixa", async () => {
  const { atualizacao, contagem } = montar();
  const e = await atualizacao.procurar();
  assert.equal(e.estado, "disponivel");
  assert.equal(e.versaoNova, "0.9.1");
  assert.equal(e.pagina, PAGINA_091);
  assert.equal(e.instalavel, true);
  assert.equal(contagem.buscarUltima, 1);
  assert.equal(contagem.baixar, 0);
  assert.equal(contagem.procurar, 0);
  assert.equal(contagem.criarInstalador, 0);
  await atualizacao.aoAbrir();
  assert.equal(contagem.baixar, 0);
});

test("abrir_respeita_o_ajuste", async () => {
  const desligado = montar({ config: JSON.stringify({ procurarAoAbrir: false }) });
  assert.equal(desligado.atualizacao.estado().procurarAoAbrir, false);
  await desligado.atualizacao.aoAbrir();
  assert.equal(desligado.contagem.buscarUltima, 0);
  assert.equal(desligado.atualizacao.estado().estado, "parado");

  const ligado = montar({ config: JSON.stringify({ procurarAoAbrir: true }) });
  // Antes da procura de abertura, o estado já avisa que ela vem (a tela começa a acompanhar).
  assert.equal(ligado.atualizacao.estado().estado, "procurando");
  await ligado.atualizacao.aoAbrir();
  assert.equal(ligado.contagem.buscarUltima, 1);
  assert.equal(ligado.atualizacao.estado().estado, "disponivel");
  await ligado.atualizacao.aoAbrir();
  assert.equal(ligado.contagem.buscarUltima, 1);
});

test("abrir_com_falha_vira_erro_sem_lancar", async () => {
  const { atualizacao } = montar({
    release: () => {
      throw new Error("sem rede");
    },
  });
  const e = await atualizacao.aoAbrir();
  assert.equal(e.estado, "erro");
  assert.equal(e.erro.code, "ATUALIZACAO_FALHOU");
  assert.equal(e.erro.message, "Não foi possível procurar atualizações. Tente de novo mais tarde.");
});

test("procurar_nao_aceita_segunda_chamada_em_curso", async () => {
  let soltar;
  const { atualizacao, contagem } = montar({
    release: () => new Promise((r) => (soltar = () => r({ tag_name: "v0.9.1", html_url: PAGINA_091 }))),
  });
  const primeira = atualizacao.procurar();
  const segunda = await atualizacao.procurar();
  assert.equal(segunda.estado, "procurando");
  assert.equal(contagem.buscarUltima, 1);
  soltar();
  assert.equal((await primeira).estado, "disponivel");
});

test("nao_instala_em_dmg_e_deb", async () => {
  for (const formato of ["dmg", "deb"]) {
    const { atualizacao, contagem } = montar({ formato });
    const e = await atualizacao.procurar();
    assert.equal(e.estado, "disponivel");
    assert.equal(e.instalavel, false);
    await assert.rejects(atualizacao.baixar(), (err) => {
      assert.equal(err.code, "ATUALIZACAO_NAO_INSTALAVEL");
      assert.equal(err.message, "Neste sistema o app não se atualiza sozinho: baixe o instalador novo na página da versão.");
      return true;
    });
    assert.equal(contagem.criarInstalador, 0);
    assert.equal(contagem.baixar, 0);
    assert.equal(atualizacao.estado().estado, "disponivel");
  }
});

test("nsis_baixa_e_reinicia_so_por_clique", async () => {
  for (const formato of ["nsis", "appimage"]) {
    const { atualizacao, contagem, vistos } = montar({ formato });
    await assert.rejects(atualizacao.baixar(), comCode("ATUALIZACAO_FALHOU"));
    await atualizacao.procurar();
    assert.throws(() => atualizacao.reiniciar(), comCode("ATUALIZACAO_FALHOU"));
    const fim = await atualizacao.baixar();
    assert.equal(contagem.procurar, 1);
    assert.equal(contagem.baixar, 1);
    assert.deepEqual(
      vistos.map((e) => [e.estado, e.progresso]),
      [
        ["baixando", 0],
        ["baixando", 50],
        ["baixando", 100],
      ],
    );
    assert.equal(fim.estado, "baixada");
    assert.equal(fim.versaoNova, "0.9.1");
    assert.equal(contagem.instalar, 0);
    const r = atualizacao.reiniciar();
    assert.equal(contagem.instalar, 1);
    assert.equal(r.estado, "baixada");
    assert.equal(r.reiniciando, true);
  }
});

test("latest_yml_com_outra_versao", async () => {
  const { atualizacao, contagem } = montar({ versaoDoYml: "0.9.2" });
  await atualizacao.procurar();
  await assert.rejects(atualizacao.baixar(), comCode("ATUALIZACAO_FALHOU"));
  assert.equal(contagem.procurar, 1);
  assert.equal(contagem.baixar, 0);
  // A versão nova continua oferecida, com o erro, para tentar de novo.
  const e = atualizacao.estado();
  assert.equal(e.estado, "disponivel");
  assert.equal(e.versaoNova, "0.9.1");
  assert.equal(e.progresso, null);
  assert.equal(e.erro.code, "ATUALIZACAO_FALHOU");
});

test("pagina_fora_do_repositorio_vira_a_padrao", () => {
  for (const url of [
    "https://github.com/outro/Guru-de-Investimentos/releases/tag/v0.9.1",
    "https://github.com/guilhermebecko/Guru-de-Investimentos.evil.com/releases/x",
    "http://github.com/guilhermebecko/Guru-de-Investimentos/releases/tag/v0.9.1",
    "javascript:alert(1)",
    undefined,
  ]) {
    assert.deepEqual(lerRelease({ tag_name: "v0.9.1", html_url: url }, "0.9.0"), { versao: "0.9.1", pagina: PAGINA_PADRAO });
  }
  assert.deepEqual(lerRelease({ tag_name: "v0.9.1", html_url: PAGINA_091 }, "0.9.0"), { versao: "0.9.1", pagina: PAGINA_091 });
});

test("tag_invalida_e_erro", async () => {
  for (const tag of ["v1.0.0-beta", "1.0.0", "latest", "v1.0", "", undefined]) {
    assert.throws(() => lerRelease({ tag_name: tag, html_url: PAGINA_091 }, "0.9.0"), comCode("ATUALIZACAO_FALHOU"));
  }
  assert.throws(() => lerRelease(null, "0.9.0"), comCode("ATUALIZACAO_FALHOU"));
  const { atualizacao } = montar({ release: { tag_name: "v1.0.0-beta", html_url: PAGINA_091 } });
  await assert.rejects(atualizacao.procurar(), comCode("ATUALIZACAO_FALHOU"));
  assert.equal(atualizacao.estado().estado, "erro");
});

test("versao_igual_ou_menor_e_atualizado", async () => {
  for (const tag of ["v0.9.0", "v0.8.9"]) {
    const { atualizacao } = montar({ release: { tag_name: tag, html_url: PAGINA_091 } });
    const e = await atualizacao.procurar();
    assert.equal(e.estado, "atualizado");
    assert.equal(e.versaoNova, null);
    await assert.rejects(atualizacao.baixar(), comCode("ATUALIZACAO_FALHOU"));
  }
  // Sem nenhuma release publicada (a API responde 404 e buscarUltima devolve null).
  const { atualizacao } = montar({ release: null });
  assert.equal((await atualizacao.procurar()).estado, "atualizado");
});

test("dev_sem_teste_nao_consulta", async () => {
  const { atualizacao, contagem } = montar({ formato: "dev" });
  assert.equal(atualizacao.estado().estado, "indisponivel");
  await atualizacao.aoAbrir();
  await assert.rejects(atualizacao.procurar(), (e) => {
    assert.equal(e.code, "ATUALIZACAO_INDISPONIVEL");
    assert.equal(e.message, "A procura de atualizações só funciona no app instalado.");
    return true;
  });
  await assert.rejects(atualizacao.baixar(), comCode("ATUALIZACAO_INDISPONIVEL"));
  assert.equal(contagem.buscarUltima, 0);
  assert.equal(contagem.criarInstalador, 0);
  assert.equal(atualizacao.estado().estado, "indisponivel");
});

test("formato_instalado", () => {
  assert.equal(formatoInstalado({ empacotado: true, plataforma: "win32", env: {} }), "nsis");
  assert.equal(formatoInstalado({ empacotado: true, plataforma: "darwin", env: {} }), "dmg");
  assert.equal(
    formatoInstalado({ empacotado: true, plataforma: "linux", env: { APPIMAGE: "/x/Guru.AppImage", APPDIR: "/tmp/.mount_Guru" }, execPath: "/tmp/.mount_Guru/guru" }),
    "appimage",
  );
  // APPIMAGE herdado de outro AppImage, com o app rodando do .deb: não é AppImage.
  assert.equal(
    formatoInstalado({ empacotado: true, plataforma: "linux", env: { APPIMAGE: "/x/Outro.AppImage", APPDIR: "/tmp/.mount_Outro" }, execPath: "/opt/Guru de Investimentos/guru" }),
    "deb",
  );
  assert.equal(formatoInstalado({ empacotado: true, plataforma: "linux", env: { APPIMAGE: "/x/Guru.AppImage" }, execPath: "/opt/x/guru" }), "deb");
  assert.equal(formatoInstalado({ empacotado: true, plataforma: "linux", env: {} }), "deb");
  assert.equal(formatoInstalado({ empacotado: false, plataforma: "win32", env: {} }), "dev");
});

test("config_padrao_ligada_e_arquivo_ilegivel", async () => {
  const sem = montar();
  assert.equal(sem.atualizacao.estado().procurarAoAbrir, true);
  assert.equal(fs.existsSync(sem.arquivoConfig), false);

  const ilegivel = montar({ config: "{ isto não é json" });
  assert.equal(ilegivel.atualizacao.estado().procurarAoAbrir, true);
  const e = await ilegivel.atualizacao.salvarConfig({ procurarAoAbrir: false });
  assert.equal(e.procurarAoAbrir, false);
  assert.deepEqual(JSON.parse(fs.readFileSync(ilegivel.arquivoConfig, "utf8")), { procurarAoAbrir: false });

  const outroTipo = montar({ config: JSON.stringify({ procurarAoAbrir: "não" }) });
  assert.equal(outroTipo.atualizacao.estado().procurarAoAbrir, true);

  await assert.rejects(ilegivel.atualizacao.salvarConfig({ procurarAoAbrir: "sim" }), comCode("ATUALIZACAO_CONFIG_INVALIDA"));
});

test("estado_tem_o_formato_do_contrato", () => {
  const { atualizacao } = montar({ config: JSON.stringify({ procurarAoAbrir: false }) });
  assert.deepEqual(atualizacao.estado(), {
    estado: "parado",
    versaoAtual: "0.9.0",
    versaoNova: null,
    pagina: null,
    instalavel: true,
    formato: "nsis",
    procurarAoAbrir: false,
    progresso: null,
    erro: null,
    reiniciando: false,
  });
});

// --- atualizacao-electron.js: a busca real (com net.fetch falso) e o feed de teste ---

function respostaFalsa({ status = 200, corpo = "{}" } = {}) {
  const bytes = Buffer.from(corpo);
  return {
    ok: status >= 200 && status < 300,
    status,
    body: new ReadableStream({
      start(c) {
        c.enqueue(new Uint8Array(bytes));
        c.close();
      },
    }),
  };
}

test("buscar_ultima_consulta_a_api_com_os_cabecalhos", async () => {
  const pedidos = [];
  const net = {
    fetch: async (url, opcoes) => {
      pedidos.push([url, opcoes]);
      return respostaFalsa({ corpo: JSON.stringify({ tag_name: "v0.9.1", html_url: PAGINA_091 }) });
    },
  };
  const json = await criarBuscarUltima({ net, versaoApp: "0.9.0" })();
  assert.deepEqual(json, { tag_name: "v0.9.1", html_url: PAGINA_091 });
  assert.equal(pedidos.length, 1);
  assert.equal(pedidos[0][0], "https://api.github.com/repos/guilhermebecko/Guru-de-Investimentos/releases/latest");
  assert.deepEqual(pedidos[0][1].headers, { Accept: "application/vnd.github+json", "User-Agent": "GuruDeInvestimentos/0.9.0" });
  assert.ok(pedidos[0][1].signal instanceof AbortSignal);
});

test("buscar_ultima_404_e_null_e_outros_erros_falham", async () => {
  const com = (r) => criarBuscarUltima({ net: { fetch: async () => r }, versaoApp: "0.9.0" });
  assert.equal(await com(respostaFalsa({ status: 404 }))(), null);
  await assert.rejects(com(respostaFalsa({ status: 500 }))(), comCode("ATUALIZACAO_FALHOU"));
  await assert.rejects(com(respostaFalsa({ corpo: "não é json" }))(), comCode("ATUALIZACAO_FALHOU"));
  await assert.rejects(com(respostaFalsa({ corpo: "x".repeat(1024 * 1024 + 1) }))(), comCode("ATUALIZACAO_FALHOU"));
  const semRede = criarBuscarUltima({
    net: {
      fetch: async () => {
        throw new Error("net::ERR_INTERNET_DISCONNECTED /caminho/qualquer");
      },
    },
    versaoApp: "0.9.0",
  });
  await assert.rejects(semRede(), (e) => e.code === "ATUALIZACAO_FALHOU" && !e.message.includes("/caminho"));
});

test("feed_de_teste_le_o_arquivo_e_o_instalador_falso_nao_fecha", async () => {
  const dir = pastaTemp();
  const arquivo = path.join(dir, "feed.json");
  fs.writeFileSync(arquivo, JSON.stringify({ formato: "nsis", ultima: { tag: "v9.9.0", html_url: PAGINA_091 }, erro: false }));
  const feed = criarFeedTeste(arquivo, { intervaloMs: 0 });
  assert.equal(feed.formato, "nsis");
  assert.deepEqual(await feed.buscarUltima(), { tag_name: "v9.9.0", html_url: PAGINA_091 });
  const instalador = feed.criarInstalador();
  assert.equal(await instalador.procurar(), "9.9.0");
  const progressos = [];
  await instalador.baixar((p) => progressos.push(p));
  assert.deepEqual(progressos, [0, 50, 100]);
  instalador.instalar();

  fs.writeFileSync(arquivo, JSON.stringify({ formato: "deb", ultima: null, erro: true }));
  const comErro = criarFeedTeste(arquivo, { intervaloMs: 0 });
  assert.equal(comErro.formato, "deb");
  await assert.rejects(comErro.buscarUltima(), comCode("ATUALIZACAO_FALHOU"));

  fs.writeFileSync(arquivo, JSON.stringify({ formato: "dev", ultima: null, erro: false }));
  assert.throws(() => criarFeedTeste(arquivo), /formato/);
});
