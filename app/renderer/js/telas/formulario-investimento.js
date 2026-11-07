// Formulário de novo e de editar investimento: os campos mudam conforme o tipo escolhido.
import { api } from "../api.js";
import { atualizarMercado } from "../mercado.js";
import { formatarData, formatarReais, paraCentavos, paraTaxa } from "../formato.js";
import { CAMPOS_DO_TIPO as Q, legendaRendimento, montarDados } from "./formulario-investimento/dados.js";

const OUTRA = "__outra";
export const ROTULO_TAXA = {
  pos: ["Rendimento", "%"],
  pre: ["Taxa prefixada", "% a.a."],
  ipca: ["Taxa acima da inflação", "% a.a."],
  selic: ["Taxa acima da Selic", "% a.a."],
};

function el(tag, texto, classe) {
  const e = document.createElement(tag);
  if (texto !== undefined) e.textContent = texto;
  if (classe) e.className = classe;
  return e;
}

function opcao(valor, texto) {
  const o = el("option", texto);
  o.value = valor;
  return o;
}

function select(id, opcoes) {
  const s = el("select");
  s.id = id;
  s.append(...opcoes.map((o) => opcao(o.codigo, o.rotulo)));
  return s;
}

function entrada(id, tipo, extra = {}) {
  const i = el("input");
  i.id = id;
  i.type = tipo;
  Object.assign(i, extra);
  return i;
}

const semPrefixo = (centavos) => formatarReais(centavos).replace(/^R\$ /, "");

function montar(raiz, catalogo, vigente, { id, inicial, metas }) {
  const edicao = id !== undefined;
  const tipos = new Map(catalogo.tipos.map((t) => [t.codigo, t]));
  const rotulo = (lista, codigo) => lista.find((o) => o.codigo === codigo)?.rotulo ?? codigo;

  const form = el("form", undefined, "card form");
  form.noValidate = true;
  const geral = el("div", undefined, "faixa");
  geral.setAttribute("role", "alert");
  geral.hidden = true;
  const aviso = el("div", inicial.aviso, "faixa ok");
  aviso.setAttribute("role", "status");
  aviso.hidden = !inicial.aviso;

  // campos: nome -> { wrap, quando, controle(), erro, base }
  const campos = {};
  const secoes = [];

  function registrar(nome, rot, controle, { quando = () => true, classe = "", dica, afixo, extra } = {}) {
    const wrap = el("div", undefined, `field ${classe}`.trim());
    const label = el("label", rot);
    label.htmlFor = controle.id;
    const erro = el("span", undefined, "erro");
    erro.id = `e-${nome}`;
    erro.hidden = true;
    const corpo = afixo ?? controle;
    wrap.append(label, corpo);
    if (extra) wrap.append(...extra);
    if (dica) wrap.append(dica);
    wrap.append(erro);
    campos[nome] = { wrap, quando, label, erro, controle: () => controle, base: dica ? [dica.id] : [] };
    return controle;
  }

  function secao(titulo, quando, ...nomes) {
    const cab = el("div", titulo, "formsec");
    const grade = el("div", undefined, "grid4");
    grade.append(...nomes.map((n) => campos[n].wrap));
    secoes.push({ cab, grade, quando, nomes });
    return [cab, grade];
  }

  const rf = (c) => c.tipo.forma === "renda_fixa";
  const rv = (c) => c.tipo.forma === "renda_variavel";

  // --- controles ---
  const tipo = el("select");
  tipo.id = "f-tipo";
  for (const grupo of [...new Set(catalogo.tipos.map((t) => t.grupo))]) {
    const og = el("optgroup");
    og.label = grupo;
    og.append(...catalogo.tipos.filter((t) => t.grupo === grupo).map((t) => opcao(t.codigo, t.rotulo)));
    tipo.append(og);
  }
  const emissor = entrada("f-emissor", "text");
  const cnpj = entrada("f-cnpj", "text", { placeholder: "00.000.000/0000-00", className: "num" });
  cnpj.inputMode = "numeric";
  const dicaCnpj = el("span", "Com o CNPJ, o valor atual vem da cota diária publicada pela CVM.", "hint");
  dicaCnpj.id = "f-cnpj-dica";
  cnpj.setAttribute("aria-describedby", dicaCnpj.id);
  const ticker = entrada("f-ticker", "text", { placeholder: "ex.: ITUB4", className: "caixa-alta" });
  const instituicao = select("f-instituicao", [
    ...catalogo.instituicoes.map((n) => ({ codigo: n, rotulo: n })),
    { codigo: OUTRA, rotulo: "Outra…" },
  ]);
  const instituicaoOutra = entrada("f-instituicao-outra", "text");
  instituicaoOutra.setAttribute("aria-label", "Nome da instituição");
  const indexador = el("select");
  indexador.id = "f-indexador";
  const taxa = entrada("f-taxa", "text", { className: "num" });
  taxa.inputMode = "decimal";
  const sufixo = el("span");
  const benchmark = select("f-benchmark", catalogo.benchmarks);
  benchmark.setAttribute("aria-label", "Benchmark");
  const dicaTaxa = el("span", undefined, "hint");
  dicaTaxa.id = "f-taxa-dica";
  taxa.setAttribute("aria-describedby", dicaTaxa.id);
  const quantidade = entrada("f-quantidade", "number", { step: "1", min: "0", className: "num", placeholder: "150" });
  const preco = entrada("f-preco", "text", { className: "num", placeholder: "31,42" });
  preco.inputMode = "decimal";
  const data = entrada("f-data", "date");
  const vencimento = entrada("f-vencimento", "date");
  const valor = entrada("f-valor", "text", { className: "num" });
  valor.inputMode = "decimal";
  const liquidez = select("f-liquidez", catalogo.liquidezes);
  const carencia = entrada("f-carencia", "date");
  const ir = select("f-ir", catalogo.irs);
  const garantia = el("output", undefined, "fixo");
  garantia.id = "f-garantia";
  const observacao = entrada("f-observacao", "text");
  const meta = el("select");
  meta.id = "f-meta";
  meta.append(opcao("", "Nenhuma"), ...metas.map((m) => opcao(String(m.id), m.nome)));

  const marca = (id) => {
    const m = el("span", "preenchido pelo tipo", "auto");
    m.id = id;
    return m;
  };
  const marcaIr = marca("f-ir-auto");
  const marcaGarantia = marca("f-garantia-auto");
  const prefixo = (controle) => {
    const a = el("div", undefined, "affix");
    a.append(el("span", "R$"), controle);
    return a;
  };
  const taxaComSufixo = () => {
    const a = el("div", undefined, "affix");
    a.append(taxa, sufixo);
    const r = el("div", undefined, "rate");
    r.append(a, benchmark);
    return r;
  };

  registrar("tipo", "Tipo", tipo);
  registrar("emissor", "Emissor", emissor, { classe: "span2", quando: Q.emissor });
  registrar("cnpj", "CNPJ do fundo (opcional)", cnpj, { classe: "span2", quando: Q.cnpj, dica: dicaCnpj });
  registrar("ticker", "Ticker", ticker, { classe: "span2", quando: Q.ticker });
  registrar("instituicao", "Corretora ou banco", instituicao, { extra: [instituicaoOutra] });
  registrar("indexador", "Indexador", indexador, { quando: Q.indexador });
  registrar("taxa", "Rendimento", taxa, { quando: Q.taxa, dica: dicaTaxa, afixo: taxaComSufixo() });
  registrar("quantidade", "Quantidade", quantidade, { quando: Q.quantidade });
  registrar("precoMedioCentavos", "Preço médio", preco, { quando: Q.precoMedioCentavos, afixo: prefixo(preco) });
  registrar("dataAplicacao", "Data da aplicação", data);
  registrar("vencimento", "Vencimento", vencimento, { quando: Q.vencimento });
  registrar("valorAplicadoCentavos", "Valor aplicado", valor, { quando: Q.valorAplicadoCentavos, afixo: prefixo(valor) });
  registrar("liquidez", "Liquidez", liquidez, { quando: Q.liquidez });
  registrar("dataCarencia", "Data da carência", carencia, { quando: Q.dataCarencia });
  registrar("ir", "Imposto de renda", ir, { quando: Q.ir, extra: [marcaIr] });
  registrar("garantia", "Garantia", garantia, { quando: Q.garantia, extra: [marcaGarantia] });
  registrar("metaId", "Vincular a uma meta", meta, { classe: "span2" });
  registrar("observacao", "Observação (opcional)", observacao, { classe: "span3" });
  // O benchmark não tem campo próprio: o erro dele aparece junto da taxa.
  campos.benchmark = { ...campos.taxa, controle: () => benchmark, base: [] };
  campos.benchmark.erro = campos.taxa.erro;

  const resumo = el("small");
  const botaoOutro = el("button", "Salvar e adicionar outro", "btn ghost");
  botaoOutro.type = "button";
  botaoOutro.hidden = edicao;
  const botaoSalvar = el("button", "Salvar", "btn");
  botaoSalvar.type = "submit";
  const acoes = el("span", undefined, "row");
  acoes.append(botaoOutro, botaoSalvar);
  const rodape = el("div", undefined, "row");
  rodape.append(resumo, acoes);

  form.append(
    geral,
    aviso,
    ...secao("Identificação", () => true, "tipo", "emissor", "cnpj", "ticker", "instituicao"),
    ...secao("Rentabilidade e prazo", rf, "indexador", "taxa"),
    ...secao("Posição", rv, "quantidade", "precoMedioCentavos"),
    ...secao("Valores e condições", () => true, "dataAplicacao", "vencimento", "valorAplicadoCentavos", "liquidez", "dataCarencia", "ir", "garantia", "metaId", "observacao"),
    rodape,
  );

  // --- comportamento ---
  const tipoAtual = () => tipos.get(tipo.value);
  const contexto = () => ({ tipo: tipoAtual(), liquidez: liquidez.value });

  function ajustarOpcoes(sel, lista, atual) {
    sel.replaceChildren(...lista.map((o) => opcao(o.codigo, o.rotulo)));
    sel.value = lista.some((o) => o.codigo === atual) ? atual : lista[0].codigo;
  }

  let anterior = null;
  function aoTrocarTipo() {
    const t = tipoAtual();
    // Saindo de um valor forçado, volta ao padrão em vez de deixá-lo selecionado.
    if (anterior?.indexadorForcado && !t.indexadorForcado) indexador.value = "pos";
    // Ao mudar para um tipo com outra liquidez padrão, ela vem do catálogo.
    if (t.liquidezPadrao && anterior?.liquidezPadrao !== t.liquidezPadrao) liquidez.value = t.liquidezPadrao;
    anterior = t;
    // "Selic + taxa" só existe forçado no Tesouro Selic.
    const lista = t.indexadorForcado ? catalogo.indexadores : catalogo.indexadores.filter((o) => o.codigo !== "selic");
    ajustarOpcoes(indexador, lista, t.indexadorForcado ?? indexador.value);
    indexador.disabled = !!t.indexadorForcado;
    if (t.liquidezForcada) liquidez.value = t.liquidezForcada;
    liquidez.disabled = !!t.liquidezForcada;
    if (t.irPadrao) ir.value = t.irPadrao;
    ir.disabled = t.forma === "renda_fixa" && !t.irEditavel;
    marcaIr.hidden = !ir.disabled;
    garantia.textContent = t.garantia ? rotulo(catalogo.garantias, t.garantia) : "";
    marcaGarantia.hidden = !t.garantia;
    campos.emissor.label.textContent = t.forma === "valor" ? "Nome" : "Emissor";
    campos.dataAplicacao.label.textContent = t.forma === "renda_variavel" ? "Data da primeira compra" : "Data da aplicação";
    atualizar();
  }

  function atualizar() {
    const c = contexto();
    for (const campo of Object.values(campos)) campo.wrap.hidden = !campo.quando(c);
    for (const s of secoes) {
      const visivel = s.quando(c);
      s.cab.hidden = s.grade.hidden = !visivel;
    }
    instituicaoOutra.hidden = instituicao.value !== OUTRA;
    benchmark.hidden = indexador.value !== "pos";
    const [rot, suf] = ROTULO_TAXA[indexador.value] ?? ROTULO_TAXA.pos;
    campos.taxa.label.textContent = rot;
    sufixo.textContent = suf;
    const legenda = legendaRendimento(indexador.value, paraTaxa(taxa.value) ?? "", rotulo(catalogo.benchmarks, benchmark.value));
    dicaTaxa.textContent = legenda ? `Lê-se: ${legenda}` : "";
    resumo.textContent = textoResumo(c, legenda);
  }

  function textoResumo(c, legenda) {
    const t = c.tipo;
    const nome = t.forma === "renda_variavel" ? ticker.value.trim().toUpperCase() : c.tipo.exigeEmissor ? emissor.value.trim() : "";
    const partes = [[t.rotulo, nome].filter(Boolean).join(" ")];
    if (rf(c) && legenda) partes.push(legenda);
    if (!rv(c)) partes.push(vencimento.value ? `vence em ${formatarData(vencimento.value)}` : "sem vencimento");
    return partes.join(" · ");
  }

  function limparErros() {
    geral.hidden = true;
    for (const campo of Object.values(campos)) {
      campo.erro.hidden = true;
      campo.erro.textContent = "";
      const ctl = campo.controle();
      ctl.removeAttribute("aria-invalid");
      if (campo.base.length) ctl.setAttribute("aria-describedby", campo.base.join(" "));
      else ctl.removeAttribute("aria-describedby");
    }
  }

  function marcarErros(erros) {
    const soltos = [];
    let primeiro = null;
    for (const { campo: nome, mensagem } of erros) {
      const campo = campos[nome === "instituicao" && instituicao.value === OUTRA ? "instituicao" : nome];
      const ctl = campo?.controle();
      if (!campo || campo.wrap.hidden) {
        soltos.push(mensagem);
        continue;
      }
      const alvo = nome === "instituicao" && instituicao.value === OUTRA ? instituicaoOutra : ctl;
      campo.erro.textContent = mensagem;
      campo.erro.hidden = false;
      alvo.setAttribute("aria-invalid", "true");
      alvo.setAttribute("aria-describedby", [...campo.base, campo.erro.id].join(" "));
      const antes = primeiro && primeiro.compareDocumentPosition(alvo) & Node.DOCUMENT_POSITION_PRECEDING;
      if (!primeiro || antes) primeiro = alvo;
    }
    if (soltos.length) {
      geral.textContent = soltos.join(" ");
      geral.hidden = false;
    }
    (primeiro ?? (soltos.length ? geral : null))?.focus?.();
  }

  function valores() {
    return {
      instituicao: instituicao.value === OUTRA ? instituicaoOutra.value : instituicao.value,
      emissor: emissor.value, cnpj: cnpj.value, ticker: ticker.value, indexador: indexador.value, taxa: taxa.value,
      benchmark: benchmark.value, dataAplicacao: data.value, vencimento: vencimento.value,
      valor: valor.value, quantidade: quantidade.value, preco: preco.value, liquidez: liquidez.value,
      dataCarencia: carencia.value, ir: ir.value, observacao: observacao.value, metaId: meta.value,
    };
  }

  let salvando = false;
  async function salvar(outro) {
    if (salvando) return;
    limparErros();
    aviso.hidden = true;
    const { dados, erros: formato } = montarDados(tipoAtual(), valores());
    if (formato.length) return marcarErros(formato);
    salvando = true;
    botaoSalvar.disabled = botaoOutro.disabled = true;
    try {
      const { erros = [] } = edicao ? await api.investimentos.atualizar(id, dados) : await api.investimentos.criar(dados);
      // Salvou: busca em segundo plano o valor atual do que foi lançado.
      if (erros.length === 0) atualizarMercado();
      if (!vigente()) return;
      if (erros.length) return marcarErros(erros);
      if (outro) {
        montar(raiz, catalogo, vigente, {
          inicial: { dados: { tipo: dados.tipo, instituicao: dados.instituicao, metaId: dados.metaId }, aviso: "Investimento salvo." },
          metas,
        });
        // O que some é o contêiner do campo (`.field`), não o input.
        raiz.querySelector(".field:not([hidden]) > #f-emissor, .field:not([hidden]) > #f-ticker")?.focus();
      } else {
        window.location.hash = "#/investimentos";
      }
    } catch (e) {
      if (!vigente()) return;
      console.error("Falha ao salvar investimento:", e);
      marcarErros([{ campo: "geral", mensagem: e.code === "NAO_ENCONTRADO" ? e.message : "Não foi possível salvar o investimento." }]);
    } finally {
      salvando = false;
      botaoSalvar.disabled = botaoOutro.disabled = false;
    }
  }

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    salvar(false);
  });
  botaoOutro.addEventListener("click", () => salvar(true));
  tipo.addEventListener("change", aoTrocarTipo);
  form.addEventListener("input", atualizar);
  form.addEventListener("change", atualizar);
  for (const campoReais of [valor, preco]) {
    campoReais.addEventListener("blur", () => {
      const c = paraCentavos(campoReais.value);
      if (c !== null) campoReais.value = semPrefixo(c);
    });
  }

  // --- valores iniciais ---
  const d = inicial.dados ?? {};
  tipo.value = d.tipo && tipos.has(d.tipo) ? d.tipo : catalogo.tipos[0].codigo;
  const conhecida = catalogo.instituicoes.includes(d.instituicao);
  instituicao.value = d.instituicao === undefined || conhecida ? (d.instituicao ?? catalogo.instituicoes[0]) : OUTRA;
  if (!conhecida && d.instituicao !== undefined) instituicaoOutra.value = d.instituicao;
  aoTrocarTipo();
  const poe = (controle, v) => {
    if (v !== undefined && v !== null) controle.value = v;
  };
  if (d.indexador && !indexador.disabled) indexador.value = d.indexador;
  poe(emissor, d.emissor);
  poe(cnpj, d.cnpj);
  poe(ticker, d.ticker);
  poe(taxa, d.taxa === undefined || d.taxa === null ? d.taxa : String(d.taxa).replace(".", ","));
  poe(benchmark, d.benchmark);
  poe(data, d.dataAplicacao);
  poe(vencimento, d.vencimento);
  poe(quantidade, d.quantidade);
  poe(carencia, d.dataCarencia);
  poe(observacao, d.observacao);
  if (d.metaId != null && metas.some((m) => m.id === d.metaId)) meta.value = String(d.metaId);
  if (d.valorAplicadoCentavos != null) valor.value = semPrefixo(d.valorAplicadoCentavos);
  if (d.precoMedioCentavos != null) preco.value = semPrefixo(d.precoMedioCentavos);
  if (d.liquidez && !liquidez.disabled) liquidez.value = d.liquidez;
  if (d.ir && !ir.disabled) ir.value = d.ir;
  atualizar();

  const topo = el("div", undefined, "row");
  topo.append(el("h1", edicao ? "Editar investimento" : "Novo investimento"));
  raiz.replaceChildren(topo, form);
}

function naoEncontrado(raiz, mensagem) {
  const voltar = el("a", "Voltar para a lista", "btn ghost");
  voltar.href = "#/investimentos";
  const aviso = el("p", mensagem);
  aviso.setAttribute("role", "alert");
  raiz.replaceChildren(el("h1", "Editar investimento"), aviso, voltar);
}

export async function render(raiz, parametros, vigente) {
  const alvo = parametros[0];
  const id = alvo === "novo" ? undefined : Number(alvo);
  if (id !== undefined && !Number.isSafeInteger(id)) {
    return naoEncontrado(raiz, "Este investimento não existe mais. Atualize a lista.");
  }
  try {
    const [catalogo, metas, atual] = await Promise.all([
      api.investimentos.catalogo(),
      api.metas.listar(),
      id === undefined ? undefined : api.investimentos.ler(id).catch((e) => (e.code === "NAO_ENCONTRADO" ? null : Promise.reject(e))),
    ]);
    if (!vigente()) return;
    if (atual === null) return naoEncontrado(raiz, "Este investimento não existe mais. Atualize a lista.");
    montar(raiz, catalogo, vigente, { id, inicial: { dados: atual }, metas });
  } catch (e) {
    if (!vigente()) return;
    console.error("Falha ao abrir o formulário:", e);
    const p = el("p", "Não foi possível abrir o formulário.");
    p.setAttribute("role", "alert");
    raiz.replaceChildren(el("h1", "Investimento"), p);
  }
}
