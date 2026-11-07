// Sobre e avisos: a versão, as atualizações, os textos legais (todos de avisos.js) e os
// avisos de terceiros, que só são lidos no clique.
import { api } from "../api.js";
import { AVISOS, CARTOES_SOBRE } from "../avisos.js";
import { el } from "./_tabela.js";
import { montarSecao } from "../atualizacao.js";

const REPOSITORIO = "https://github.com/guilhermebecko/Guru-de-Investimentos";
const FALHA_TERCEIROS = "Não foi possível carregar os avisos de terceiros.";

let ultimoId = 0;

// Um cartão com título (h2) que nomeia a região, para leitores de tela e para os testes.
function secao(titulo) {
  const s = el("section", undefined, "card");
  const h = el("h2", titulo);
  h.id = `sobre-${++ultimoId}`;
  s.setAttribute("aria-labelledby", h.id);
  const topo = el("div", undefined, "card-title");
  topo.append(h);
  s.append(topo);
  return s;
}

function blocoApp(vigente) {
  const s = secao("Guru de Investimentos");
  const versoes = el("p", "Versão … · Núcleo …", "mono");
  const link = el("a", REPOSITORIO.replace(/^https:\/\//, ""));
  link.href = REPOSITORIO;
  s.append(versoes, link, el("p", "Licença MIT"));
  api.app
    .info()
    .then((info) => {
      if (!vigente()) return;
      versoes.textContent = `Versão ${info.versaoApp} · Núcleo ${info.coreVersion}`;
    })
    .catch((e) => {
      console.error("Falha ao ler as versões:", e);
      if (!vigente()) return;
      versoes.textContent = "Versão — · Núcleo —";
    });
  return s;
}

// O ajuste, "Procurar agora", a linha do estado e os botões da versão nova (ver atualizacao.js).
function blocoAtualizacoes(vigente) {
  const s = secao("Atualizações");
  const lugar = el("div");
  lugar.dataset.testid = "sobre-atualizacoes";
  s.append(lugar);
  montarSecao(lugar, vigente);
  return s;
}

function blocoTerceiros(vigente) {
  const s = secao("Avisos de terceiros");
  const botao = el("button", "Ver avisos de terceiros", "btn ghost");
  botao.type = "button";
  const erro = el("p", undefined, "erro");
  erro.setAttribute("role", "alert");
  erro.hidden = true;
  botao.addEventListener("click", async () => {
    botao.disabled = true;
    erro.hidden = true;
    try {
      const texto = await api.app.avisosTerceiros();
      if (!vigente()) return;
      const pre = el("pre", texto, "terceiros");
      pre.tabIndex = 0;
      botao.replaceWith(pre);
    } catch (e) {
      console.error("Falha ao carregar os avisos de terceiros:", e);
      if (!vigente()) return;
      erro.textContent = FALHA_TERCEIROS;
      erro.hidden = false;
      botao.disabled = false;
    }
  });
  s.append(botao, erro);
  return s;
}

export function render(raiz, _parametros, vigente) {
  const avisos = el("div", undefined, "sobre-avisos");
  avisos.append(
    ...CARTOES_SOBRE.map(([titulo, chave]) => {
      const c = secao(titulo);
      c.append(el("p", AVISOS[chave]));
      return c;
    }),
  );
  const pagina = el("div", undefined, "sobre");
  pagina.append(
    blocoApp(vigente),
    blocoAtualizacoes(vigente),
    avisos,
    el("div", AVISOS.REVISAO_JURIDICA, "faixa"),
    blocoTerceiros(vigente),
  );
  raiz.replaceChildren(el("h1", "Sobre e avisos"), pagina);
}
