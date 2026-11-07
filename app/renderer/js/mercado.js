// Estado da última atualização dos dados de mercado e o indicador do rodapé do menu.
import { api } from "./api.js";

function el(tag, texto, classe) {
  const e = document.createElement(tag);
  if (texto !== undefined) e.textContent = texto;
  if (classe) e.className = classe;
  return e;
}

// Menor `ultimaData` entre as fontes que têm dados, em "YYYY-MM-DD"; null se nenhuma.
// O IPCA fica de fora de propósito: ele é mensal e sua `ultimaData` é o primeiro dia do mês
// publicado (YYYY-MM-01), que sempre ficaria "atrás" das séries diárias sem indicar defasagem.
export function menorData(fontes) {
  const datas = (fontes ?? []).filter((f) => f.fonte !== "ipca" && f.fonte !== "cvm" && f.ultimaData).map((f) => f.ultimaData);
  return datas.length === 0 ? null : datas.reduce((a, b) => (b < a ? b : a));
}

// "2026-10-28" -> "28/10".
export function diaMes(iso) {
  const [, m, d] = String(iso).split("-");
  return `${d}/${m}`;
}

export function textoIndicador(fontes) {
  const data = menorData(fontes);
  return data ? `Dados de mercado de ${diaMes(data)}` : "Dados de mercado: ainda não baixados";
}

// Nota visível sob os valores: são brutos e vêm de dados de uma certa data ("dd/mm", ou null).
export function textoNota(data) {
  return `Valores brutos, antes do IR. ${data ? `Dados de mercado de ${data}.` : "Dados de mercado ainda não baixados."}`;
}

// Só as telas de leitura são redesenhadas ao fim de uma atualização; as demais (formulários,
// questionário, alocação, metas...) podem ter texto digitado ainda não enviado.
export function deveRedesenhar(hash) {
  return /^#?\/?(carteira(\/[^/]*)?|investimentos)?\/?$/.test(String(hash ?? ""));
}

// Devolve o gatilho de `executar` (que não deve lançar): chamado durante uma execução, marca um
// pedido pendente e resolve na hora; ao fim da execução em curso roda uma única execução de
// acompanhamento, por mais que tenham pedido. Só a chamada que iniciou a execução espera por tudo.
export function coalescer(executar) {
  let rodando = false;
  let pendente = false;
  return async function acionar() {
    if (rodando) {
      pendente = true;
      return;
    }
    rodando = true;
    try {
      do {
        pendente = false;
        await executar();
      } while (pendente);
    } finally {
      rodando = false;
    }
  };
}

// Aciona a mesma atualização do botão (o indicador mostra "Atualizando…"); sem efeito antes de iniciarMercado.
let acionar = () => {};
export function atualizarMercado() {
  return acionar();
}

function isoLocal(data) {
  const p = (n) => String(n).padStart(2, "0");
  return `${data.getFullYear()}-${p(data.getMonth() + 1)}-${p(data.getDate())}`;
}

// Alguma fonte em uso sem atualização feita hoje (no relógio local)? Fonte que a carteira não
// usa (sem `ultimaData`, `atualizadoEm` nem `erro`) não conta. Sem fontes em uso, sim.
export function precisaAtualizar(fontes, agora = new Date()) {
  const usadas = (fontes ?? []).filter((f) => f.ultimaData || f.atualizadoEm || f.erro);
  if (usadas.length === 0) return true;
  const hoje = isoLocal(agora);
  return usadas.some((f) => !f.atualizadoEm || isoLocal(new Date(f.atualizadoEm)) !== hoje);
}

// Status das fontes de mercado, ou null se não deu para ler.
export async function statusDoMercado() {
  try {
    return await api.mercado.status();
  } catch (e) {
    console.error("Falha ao ler o estado do mercado:", e);
    return null;
  }
}

// Data dos dados de `fontes` para notas nas telas ("dd/mm"), ou null.
export function dataDasFontes(fontes) {
  const data = menorData(fontes);
  return data ? diaMes(data) : null;
}

// Data dos dados para notas nas telas ("dd/mm"), ou null.
export async function dataDoMercado() {
  return dataDasFontes(await statusDoMercado());
}

function mostrarFalhas(fontes) {
  const dialogo = el("dialog");
  dialogo.setAttribute("aria-labelledby", "falhas-titulo");
  const lista = el("ul", undefined, "falhas");
  for (const f of fontes.filter((x) => x.erro)) {
    const li = el("li");
    li.append(el("b", f.rotulo), el("span", f.erro));
    lista.append(li);
  }
  const fechar = el("button", "Fechar", "btn ghost");
  fechar.type = "button";
  fechar.addEventListener("click", () => dialogo.close());
  const acoes = el("div", undefined, "acoes");
  acoes.append(fechar);
  const caixa = el("div", undefined, "caixa");
  const titulo = el("h2", "Fontes com falha");
  titulo.id = "falhas-titulo";
  caixa.append(titulo, lista, acoes);
  dialogo.append(caixa);
  dialogo.addEventListener("close", () => dialogo.remove());
  document.body.append(dialogo);
  dialogo.showModal();
}

export function iniciarMercado(raiz) {
  let fontes = [];
  let atualizando = false;

  function desenhar() {
    const texto = el("span", textoIndicador(fontes));
    texto.dataset.testid = "mercado-texto";
    const partes = [texto];
    if (fontes.some((f) => f.erro)) {
      const marca = el("button", "com falhas", "falhas-marca");
      marca.type = "button";
      marca.addEventListener("click", () => mostrarFalhas(fontes));
      partes.push(marca);
    }
    const botao = el("button", atualizando ? "Atualizando…" : "Atualizar", "btn ghost sm");
    botao.type = "button";
    botao.disabled = atualizando;
    botao.addEventListener("click", atualizar);
    partes.push(botao);
    raiz.replaceChildren(...partes);
  }

  async function baixar() {
    try {
      fontes = (await api.mercado.atualizar()).fontes;
    } catch (e) {
      console.error("Falha ao atualizar os dados de mercado:", e);
      try {
        fontes = await api.mercado.status();
      } catch (e2) {
        console.error("Falha ao ler o estado do mercado:", e2);
      }
    }
  }
  // Pedidos durante uma rodada (ex.: salvar um investimento) viram uma única rodada seguinte.
  const rodar = coalescer(baixar);

  async function atualizar() {
    if (atualizando) return rodar();
    atualizando = true;
    desenhar();
    try {
      await rodar();
    } finally {
      atualizando = false;
      desenhar();
    }
    // Redesenha a tela atual com os valores novos, sem pisar em texto digitado.
    if (deveRedesenhar(window.location.hash)) window.dispatchEvent(new Event("hashchange"));
  }
  acionar = atualizar;

  desenhar();
  (async () => {
    try {
      fontes = await api.mercado.status();
    } catch (e) {
      console.error("Falha ao ler o estado do mercado:", e);
    }
    if (atualizando) return;
    desenhar();
    if (precisaAtualizar(fontes)) await atualizar();
  })();
}
