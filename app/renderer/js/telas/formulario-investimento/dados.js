// Funções puras do formulário: texto da taxa ("Lê-se") e montagem do que vai ao núcleo.
import { paraCentavos, paraTaxa } from "../../formato.js";

export const MENSAGEM_TAXA = "Informe uma taxa válida.";
export const MENSAGEM_VALOR = "Informe um valor em reais, por exemplo 1.234,56.";

// "102.50" -> "102,5"; texto que não é decimal com ponto -> null.
export function formatarTaxa(texto) {
  const t = String(texto ?? "").trim();
  if (!/^\d+(\.\d+)?$/.test(t)) return null;
  const [inteira, fracao = ""] = t.split(".");
  const frac = fracao.replace(/0+$/, "");
  return `${String(BigInt(inteira))}${frac ? `,${frac}` : ""}`;
}

// Mesmo texto que o núcleo devolve em `rendimento`. `rotuloBenchmark` é "CDI" ou "Selic".
export function legendaRendimento(indexador, taxa, rotuloBenchmark) {
  const t = formatarTaxa(taxa);
  if (t === null) return "";
  if (indexador === "pos") return `${t}% do ${rotuloBenchmark}`;
  if (indexador === "pre") return `${t}% ao ano`;
  if (indexador === "ipca") return `IPCA + ${t}% ao ano`;
  if (indexador === "selic") return `Selic + ${t}% ao ano`;
  return "";
}

const vazio = (s) => s === undefined || s === null || String(s).trim() === "";

// Campos que cada tipo usa. `contexto` = { tipo (do catálogo), liquidez (código escolhido) }.
// O formulário mostra só estes; a importação os usa para saber o que pedir ao trocar o tipo.
const rv = (c) => c.tipo.forma === "renda_variavel";
const rf = (c) => c.tipo.forma === "renda_fixa";
const sempre = () => true;
export const CAMPOS_DO_TIPO = {
  tipo: sempre,
  emissor: (c) => !!c.tipo.exigeEmissor,
  ticker: rv,
  instituicao: sempre,
  indexador: rf,
  taxa: rf,
  quantidade: rv,
  precoMedioCentavos: rv,
  dataAplicacao: sempre,
  vencimento: (c) => !rv(c),
  valorAplicadoCentavos: (c) => !rv(c),
  liquidez: (c) => !rv(c),
  dataCarencia: (c) => !rv(c) && c.liquidez === "carencia",
  ir: rf,
  garantia: rf,
  metaId: sempre,
  observacao: sempre,
  cnpj: (c) => c.tipo.codigo === "fundo",
};

export function campoDoTipo(nome, contexto) {
  const regra = CAMPOS_DO_TIPO[nome];
  return !!(regra && contexto.tipo && regra(contexto));
}

// `tipo` vem do catálogo; `v` são os textos dos campos. Só entram os campos da forma do tipo.
// Devolve { dados, erros } — `erros` só cobre o formato do valor em reais, da taxa e da quantidade.
export function montarDados(tipo, v) {
  const dados = { tipo: tipo.codigo, instituicao: v.instituicao };
  const erros = [];
  const texto = (campo, valor) => {
    if (!vazio(valor)) dados[campo] = valor;
  };
  const reais = (campo, valor) => {
    if (vazio(valor)) return;
    const c = paraCentavos(valor);
    if (c === null) erros.push({ campo, mensagem: MENSAGEM_VALOR });
    else dados[campo] = c;
  };

  texto("dataAplicacao", v.dataAplicacao);
  texto("observacao", v.observacao);
  if (!vazio(v.metaId)) dados.metaId = Number(v.metaId);

  if (tipo.forma === "renda_variavel") {
    texto("ticker", v.ticker);
    if (!vazio(v.quantidade)) {
      const q = Number(v.quantidade);
      if (Number.isSafeInteger(q)) dados.quantidade = q;
      else erros.push({ campo: "quantidade", mensagem: "Informe a quantidade." });
    }
    reais("precoMedioCentavos", v.preco);
    return { dados, erros };
  }

  if (tipo.exigeEmissor) texto("emissor", v.emissor);
  if (tipo.codigo === "fundo") texto("cnpj", v.cnpj);
  texto("vencimento", v.vencimento);
  reais("valorAplicadoCentavos", v.valor);
  dados.liquidez = tipo.liquidezForcada ?? v.liquidez;
  if (dados.liquidez === "carencia") texto("dataCarencia", v.dataCarencia);
  if (dados.liquidez === "") delete dados.liquidez;

  if (tipo.forma === "renda_fixa") {
    dados.indexador = tipo.indexadorForcado ?? v.indexador;
    if (!vazio(v.taxa)) {
      const t = paraTaxa(v.taxa);
      if (t === null) erros.push({ campo: "taxa", mensagem: MENSAGEM_TAXA });
      else dados.taxa = t;
    }
    if (dados.indexador === "pos") texto("benchmark", v.benchmark);
    dados.ir = tipo.irEditavel ? v.ir : (tipo.irPadrao ?? v.ir);
  }
  return { dados, erros };
}
