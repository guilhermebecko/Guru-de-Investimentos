import { test } from "node:test";
import assert from "node:assert/strict";
import { analisarMarkdown } from "../../renderer/js/markdown.js";

const texto = (valor) => ({ tipo: "texto", valor });

test("parágrafos separados por linha em branco", () => {
  assert.deepEqual(analisarMarkdown("Olá!\n\nTudo bem?", []), [
    { tipo: "p", trechos: [texto("Olá!")] },
    { tipo: "p", trechos: [texto("Tudo bem?")] },
  ]);
});

test("linhas seguidas ficam no mesmo parágrafo", () => {
  assert.deepEqual(analisarMarkdown("Primeira\nsegunda", []), [{ tipo: "p", trechos: [texto("Primeira segunda")] }]);
});

test("negrito", () => {
  assert.deepEqual(analisarMarkdown("Isso é **importante** mesmo.", []), [
    { tipo: "p", trechos: [texto("Isso é "), { tipo: "negrito", valor: "importante" }, texto(" mesmo.")] },
  ]);
});

test("asteriscos sem par ficam como texto", () => {
  assert.deepEqual(analisarMarkdown("2 ** 3 e **aberto", []), [{ tipo: "p", trechos: [texto("2 ** 3 e **aberto")] }]);
});

test("lista com hífen", () => {
  assert.deepEqual(analisarMarkdown("Opções:\n\n- **Manter**: previsível.\n- Diversificar", []), [
    { tipo: "p", trechos: [texto("Opções:")] },
    {
      tipo: "ul",
      itens: [
        [{ tipo: "negrito", valor: "Manter" }, texto(": previsível.")],
        [texto("Diversificar")],
      ],
    },
  ]);
});

test("lista numerada", () => {
  assert.deepEqual(analisarMarkdown("1. Primeiro\n2. Segundo", []), [
    { tipo: "ol", itens: [[texto("Primeiro")], [texto("Segundo")]] },
  ]);
});

test("lista colada ao parágrafo começa um bloco novo", () => {
  assert.deepEqual(analisarMarkdown("Veja:\n- um", []), [
    { tipo: "p", trechos: [texto("Veja:")] },
    { tipo: "ul", itens: [[texto("um")]] },
  ]);
});

test("títulos de # a ###", () => {
  assert.deepEqual(analisarMarkdown("# Um\n## Dois\n### Três", []), [
    { tipo: "titulo", nivel: 1, trechos: [texto("Um")] },
    { tipo: "titulo", nivel: 2, trechos: [texto("Dois")] },
    { tipo: "titulo", nivel: 3, trechos: [texto("Três")] },
  ]);
});

test("marcador de fonte existente vira trecho fonte", () => {
  assert.deepEqual(analisarMarkdown("A Selic ficou em 15%[1][2].", [1, 2]), [
    { tipo: "p", trechos: [texto("A Selic ficou em 15%"), { tipo: "fonte", valor: 1 }, { tipo: "fonte", valor: 2 }, texto(".")] },
  ]);
});

test("marcador de fonte inexistente fica como texto", () => {
  assert.deepEqual(analisarMarkdown("Veja [3] e [1].", [1]), [
    { tipo: "p", trechos: [texto("Veja [3] e "), { tipo: "fonte", valor: 1 }, texto(".")] },
  ]);
});

test("<script> fica como texto literal", () => {
  const html = '<script>alert("x")</script> <b onclick="y">oi</b>';
  assert.deepEqual(analisarMarkdown(html, []), [{ tipo: "p", trechos: [texto(html)] }]);
});

test("texto vazio não gera bloco", () => {
  assert.deepEqual(analisarMarkdown("", []), []);
  assert.deepEqual(analisarMarkdown("\n\n  \n", []), []);
});

test("aceita um Set de números de fontes", () => {
  assert.deepEqual(analisarMarkdown("x[2]", new Set([2])), [{ tipo: "p", trechos: [texto("x"), { tipo: "fonte", valor: 2 }] }]);
});

test("tabela simples com alinhamento", () => {
  const [t] = analisarMarkdown("| Ativo | Valor |\n|:---|---:|\n| CDB | **R$ 10** |", []);
  assert.equal(t.tipo, "tabela");
  assert.deepEqual(t.alinhamentos, ["esquerda", "direita"]);
  assert.deepEqual(t.cabecalho, [[texto("Ativo")], [texto("Valor")]]);
  assert.deepEqual(t.linhas[0][1], [{ tipo: "negrito", valor: "R$ 10" }]);
});

test("linha de tabela sem separador fica parágrafo", () => {
  assert.deepEqual(analisarMarkdown("| Ativo | Valor |", []), [{ tipo: "p", trechos: [texto("| Ativo | Valor |")] }]);
});

test("tabela com células a menos ou a mais", () => {
  const [t] = analisarMarkdown("| A | B |\n|---|---|\n| 1 |\n| 1 | 2 | 3 |", []);
  assert.deepEqual(t.linhas, [[[texto("1")], []], [[texto("1")], [texto("2")]]]);
});

test("pipe escapado na célula", () => {
  const [t] = analisarMarkdown("| A | B |\n|---|---|\n| a \\| b | c |", []);
  assert.deepEqual(t.linhas[0], [[texto("a | b")], [texto("c")]]);
});

test("tabela sem pipes nas bordas", () => {
  const [t] = analisarMarkdown("Ativo | Valor\n--- | ---\nCDB | 10", []);
  assert.equal(t.tipo, "tabela");
  assert.deepEqual(t.linhas, [[[texto("CDB")], [texto("10")]]]);
});

test("itálico com asterisco e sublinhado", () => {
  assert.deepEqual(analisarMarkdown("*a* e _b_ e nome_de_variavel", [])[0].trechos, [
    { tipo: "italico", valor: "a" },
    texto(" e "),
    { tipo: "italico", valor: "b" },
    texto(" e nome_de_variavel"),
  ]);
});

test("código inline é literal", () => {
  assert.deepEqual(analisarMarkdown("`**x**`", [])[0].trechos, [{ tipo: "codigo", valor: "**x**" }]);
});

test("bloco de código cercado preserva linhas e indentação", () => {
  assert.deepEqual(analisarMarkdown("```js\nif (a) {\n  # x\n\n  | y\n}\n```\nfim", []), [
    { tipo: "codigo", texto: "if (a) {\n  # x\n\n  | y\n}" },
    { tipo: "p", trechos: [texto("fim")] },
  ]);
  assert.deepEqual(analisarMarkdown("```\nsem fim", []), [{ tipo: "codigo", texto: "sem fim" }]);
});

test("linha divisória, citação e títulos de nível 4 a 6", () => {
  assert.deepEqual(analisarMarkdown("---\n***\n___\n> a\n> b\n#### Q\n###### S", []), [
    { tipo: "hr" },
    { tipo: "hr" },
    { tipo: "hr" },
    { tipo: "citacao", trechos: [texto("a b")] },
    { tipo: "titulo", nivel: 4, trechos: [texto("Q")] },
    { tipo: "titulo", nivel: 6, trechos: [texto("S")] },
  ]);
});

test("link vira só o texto", () => {
  assert.deepEqual(analisarMarkdown("[site](https://x.com)[1]", [1])[0].trechos, [texto("site"), { tipo: "fonte", valor: 1 }]);
});

test("<script> em célula de tabela é texto literal", () => {
  const [t] = analisarMarkdown("| A |\n|---|\n| <script>x</script> |", []);
  assert.deepEqual(t.linhas[0][0], [texto("<script>x</script>")]);
});
