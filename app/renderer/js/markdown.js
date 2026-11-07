// Markdown mínimo das respostas da IA. `analisarMarkdown` é pura (sem DOM); `renderizarMarkdown`
// monta a árvore só com elementos criados e `textContent`. Nada vira HTML: `<script>` é texto.
//
// Bloco: {tipo: "p"|"citacao", trechos}, {tipo: "titulo", nivel: 1..6, trechos}, {tipo: "ul"|"ol", itens},
// {tipo: "tabela", alinhamentos, cabecalho, linhas}, {tipo: "codigo", texto} ou {tipo: "hr"}.
// Trecho: {tipo: "texto"|"negrito"|"italico"|"codigo", valor: string} ou {tipo: "fonte", valor: número}.
// Link vira só o texto.

const TITULO = /^(#{1,6})\s+(.*)$/;
const ITEM_UL = /^[-*]\s+(.*)$/;
const ITEM_OL = /^\d+\.\s+(.*)$/;
const HR = /^(?:-{3,}|\*{3,}|_{3,})$/;
const CITACAO = /^>\s?(.*)$/;
const CERCA = /^```/;
const SEPARADOR = /^:?-+:?$/;
// Ordem importa: código, negrito, itálico, link e marcador de fonte. O `_` só vale entre limites de palavra.
const INLINE =
  /`([^`]+)`|\*\*(\S(?:.*?\S)?)\*\*|\*(?![\s*])([^*]*?[^\s*])\*|(?<![\p{L}\p{N}_])_(?![\s_])([^_]*?[^\s_])_(?![\p{L}\p{N}_])|\[([^\]]+)\]\([^)\s]*\)|\[(\d+)\]/gu;

function trechos(texto, fontes) {
  const saida = [];
  const texto_ = (valor) => {
    if (!valor) return;
    const ultimo = saida[saida.length - 1];
    if (ultimo?.tipo === "texto") ultimo.valor += valor;
    else saida.push({ tipo: "texto", valor });
  };
  let i = 0;
  for (const m of texto.matchAll(INLINE)) {
    texto_(texto.slice(i, m.index));
    if (m[1] !== undefined) saida.push({ tipo: "codigo", valor: m[1] });
    else if (m[2] !== undefined) saida.push({ tipo: "negrito", valor: m[2] });
    else if (m[3] !== undefined || m[4] !== undefined) saida.push({ tipo: "italico", valor: m[3] ?? m[4] });
    else if (m[5] !== undefined) texto_(m[5]);
    else if (fontes.has(Number(m[6]))) saida.push({ tipo: "fonte", valor: Number(m[6]) });
    else texto_(m[0]);
    i = m.index + m[0].length;
  }
  texto_(texto.slice(i));
  return saida;
}

// Divide uma linha de tabela em células: tira as bordas `|` e respeita `\|`.
function celulas(linha) {
  let t = linha.trim();
  if (t.startsWith("|")) t = t.slice(1);
  if (t.endsWith("|") && !t.endsWith("\\|")) t = t.slice(0, -1);
  return t.split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
}

// Devolve os alinhamentos se a linha for um separador de tabela válido, senão null.
function separador(linha) {
  if (!linha.includes("-")) return null;
  const cs = celulas(linha);
  if (!cs.every((c) => SEPARADOR.test(c))) return null;
  return cs.map((c) => {
    const e = c.startsWith(":");
    const d = c.endsWith(":");
    return e && d ? "centro" : d ? "direita" : e ? "esquerda" : null;
  });
}

export function analisarMarkdown(texto, numerosDeFontes = []) {
  const fontes = new Set([...numerosDeFontes].map(Number));
  const blocos = [];
  let paragrafo = null;
  let lista = null;
  let citacao = null;
  const fechar = () => {
    if (paragrafo) blocos.push({ tipo: "p", trechos: trechos(paragrafo.join(" "), fontes) });
    if (lista) blocos.push({ tipo: lista.tipo, itens: lista.itens.map((t) => trechos(t, fontes)) });
    if (citacao) blocos.push({ tipo: "citacao", trechos: trechos(citacao.join(" "), fontes) });
    paragrafo = lista = citacao = null;
  };
  const linhas = String(texto ?? "").split(/\r?\n/);
  for (let i = 0; i < linhas.length; i++) {
    const linha = linhas[i].trim();
    if (CERCA.test(linha)) {
      fechar();
      const corpo = [];
      for (i++; i < linhas.length && !CERCA.test(linhas[i].trim()); i++) corpo.push(linhas[i]);
      blocos.push({ tipo: "codigo", texto: corpo.join("\n") });
      continue;
    }
    if (!linha) {
      fechar();
      continue;
    }
    if (linha.includes("|") && i + 1 < linhas.length) {
      const alinhamentos = separador(linhas[i + 1]);
      const cabeca = celulas(linha);
      if (alinhamentos && alinhamentos.length === cabeca.length) {
        fechar();
        const n = cabeca.length;
        const linhasTabela = [];
        for (i += 2; i < linhas.length && linhas[i].trim() && linhas[i].includes("|"); i++) {
          const cs = celulas(linhas[i]).slice(0, n);
          while (cs.length < n) cs.push("");
          linhasTabela.push(cs.map((c) => trechos(c, fontes)));
        }
        i--;
        blocos.push({
          tipo: "tabela",
          alinhamentos,
          cabecalho: cabeca.map((c) => trechos(c, fontes)),
          linhas: linhasTabela,
        });
        continue;
      }
    }
    const titulo = linha.match(TITULO);
    if (titulo) {
      fechar();
      blocos.push({ tipo: "titulo", nivel: titulo[1].length, trechos: trechos(titulo[2], fontes) });
      continue;
    }
    if (HR.test(linha)) {
      fechar();
      blocos.push({ tipo: "hr" });
      continue;
    }
    const cit = linha.match(CITACAO);
    if (cit) {
      if (!citacao) fechar();
      citacao ??= [];
      citacao.push(cit[1]);
      continue;
    }
    const ul = linha.match(ITEM_UL);
    const ol = ul ? null : linha.match(ITEM_OL);
    if (ul || ol) {
      const tipo = ul ? "ul" : "ol";
      if (paragrafo || citacao || lista?.tipo !== tipo) fechar();
      lista ??= { tipo, itens: [] };
      lista.itens.push((ul ?? ol)[1]);
      continue;
    }
    if (lista || citacao) fechar();
    paragrafo ??= [];
    paragrafo.push(linha);
  }
  fechar();
  return blocos;
}

function irParaFonte(n) {
  const alvo = document.getElementById(`fonte-${n}`);
  if (!alvo) return;
  alvo.scrollIntoView({ block: "nearest" });
  alvo.focus({ preventScroll: true });
}

function preencher(pai, lista, aoFonte) {
  for (const t of lista) {
    if (t.tipo === "negrito" || t.tipo === "italico" || t.tipo === "codigo") {
      const el = document.createElement({ negrito: "strong", italico: "em", codigo: "code" }[t.tipo]);
      el.textContent = t.valor;
      pai.append(el);
    } else if (t.tipo === "fonte") {
      const sup = document.createElement("sup");
      const a = document.createElement("a");
      a.href = `#fonte-${t.valor}`;
      a.textContent = String(t.valor);
      a.setAttribute("aria-label", `Fonte ${t.valor}`);
      // O hash é do roteador: o clique só rola até a fonte, sem trocar de tela.
      a.addEventListener("click", (e) => {
        e.preventDefault();
        (aoFonte ?? irParaFonte)(t.valor);
      });
      sup.append(a);
      pai.append(sup);
    } else {
      pai.append(document.createTextNode(t.valor));
    }
  }
}

// `aoFonte(n)`, opcional, troca o que o clique num marcador faz (o padrão rola até `#fonte-n`).
export function renderizarMarkdown(blocos, aoFonte) {
  const frag = document.createDocumentFragment();
  for (const b of blocos) {
    if (b.tipo === "ul" || b.tipo === "ol") {
      const lista = document.createElement(b.tipo);
      for (const item of b.itens) {
        const li = document.createElement("li");
        preencher(li, item, aoFonte);
        lista.append(li);
      }
      frag.append(lista);
    } else if (b.tipo === "tabela") {
      const caixa = document.createElement("div");
      caixa.className = "md-tabela";
      const tabela = document.createElement("table");
      const linhaDe = (celulas, tag) => {
        const tr = document.createElement("tr");
        celulas.forEach((c, k) => {
          const td = document.createElement(tag);
          const al = { esquerda: "left", centro: "center", direita: "right" }[b.alinhamentos[k]];
          if (al) td.style.textAlign = al;
          preencher(td, c, aoFonte);
          tr.append(td);
        });
        return tr;
      };
      const thead = document.createElement("thead");
      thead.append(linhaDe(b.cabecalho, "th"));
      const tbody = document.createElement("tbody");
      for (const l of b.linhas) tbody.append(linhaDe(l, "td"));
      tabela.append(thead, tbody);
      caixa.append(tabela);
      frag.append(caixa);
    } else if (b.tipo === "codigo") {
      const pre = document.createElement("pre");
      const code = document.createElement("code");
      code.textContent = b.texto;
      pre.append(code);
      frag.append(pre);
    } else if (b.tipo === "hr") {
      frag.append(document.createElement("hr"));
    } else if (b.tipo === "citacao") {
      const q = document.createElement("blockquote");
      preencher(q, b.trechos, aoFonte);
      frag.append(q);
    } else {
      const p = document.createElement("p");
      if (b.tipo === "titulo") {
        const forte = document.createElement("strong");
        preencher(forte, b.trechos, aoFonte);
        p.append(forte);
      } else {
        preencher(p, b.trechos, aoFonte);
      }
      frag.append(p);
    }
  }
  return frag;
}
