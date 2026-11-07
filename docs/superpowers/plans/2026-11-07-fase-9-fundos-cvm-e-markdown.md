# Fase 9: valor de fundos pela cota da CVM e respostas formatadas no Assistente — plano

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fundo com CNPJ ganha valor atual pela cota diária publicada pela CVM, e as respostas da IA aparecem formatadas (tabelas, itálico, código, citação, linha divisória).

**Architecture:** Um leitor puro do Informe Diário da CVM (`mercado/cvm.rs`) alimenta uma tabela `cota_fundo` só com os CNPJs da carteira. O atualizador ganha a fonte `cvm`, que baixa os meses necessários. A valorização do fundo projeta uma base, que é o saldo do extrato ou o valor aplicado, pela razão entre a cota do dia e a cota da data da base. No renderer, `markdown.js` ganha blocos e trechos novos, sem gerar HTML.

**Tech Stack:** Rust (`guru-core`, rusqlite, rust_decimal, zip 9 já presente), napi (`guru-node`), Electron + JS puro, Playwright.

**Spec:** este documento (seção "Design"). Origem: `docs/pendencias.md`, conversa de 2026-11-07.

## Design

### Por que
O usuário cadastrou fundos à mão e o app diz "sem fonte de preço". Hoje só o extrato da BTG dá valor a um fundo. A CVM publica a cota diária de todos os fundos por CNPJ, de graça.

### Fonte
- URL: `https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_AAAAMM.zip`.
- O arquivo é um zip com um CSV: separador `;`, ponto decimal, CRLF e cerca de 11 MB por mês fechado. Os meses começam em 2021-01, e antes disso só existem zips anuais grandes, que ficam fora.
- Há dois layouts de cabeçalho, e o leitor acha as colunas pelo nome:
  - até 2024 (aproximadamente): `TP_FUNDO;CNPJ_FUNDO;DT_COMPTC;VL_TOTAL;VL_QUOTA;...`
  - depois: `TP_FUNDO_CLASSE;CNPJ_FUNDO_CLASSE;ID_SUBCLASSE;DT_COMPTC;VL_TOTAL;VL_QUOTA;...`
- Linhas com `ID_SUBCLASSE` preenchido são cotas de subclasse e são ignoradas. Um CNPJ que só tem linhas de subclasse fica sem cota.
- O CNPJ vem como `00.000.000/0000-00` e é guardado só com os 14 dígitos.

### Regras
- **CNPJ do fundo.** Fica na coluna existente `investimentos.codigo_b3`, que é onde a importação da BTG já grava o CNPJ do fundo (`comparacao_btg.rs`).
  - O cadastro de "Fundo de investimento" ganha o campo opcional "CNPJ do fundo". Só o tipo `fundo` tem esse campo; previdência e cripto não mudam.
  - CNPJ válido: 14 dígitos com os dígitos verificadores corretos, recusando os 14 dígitos iguais.
- **Valor atual de um fundo com CNPJ válido**, na data `d`:
  - `cota_atual` = a última cota guardada com data ≤ `d`.
  - A base é o saldo do extrato (`saldo_centavos`, `saldo_data`) quando existe e `saldo_data ≤ d`. Sem saldo, a base é o valor aplicado na `data_aplicacao`.
  - `cota_base` = a última cota com data ≤ data da base.
  - Valor = `floor(base × cota_atual / cota_base)`, com origem `Cota` ("cota diária da CVM") e data de referência igual à data de `cota_atual`.
  - Quando a base é o saldo e não há cota mais nova que a da data do saldo (as duas cotas caem na mesma data), vale a regra da fase 8: o saldo, com origem `Extrato`.
  - Sem `cota_atual` ou sem `cota_base`, também vale a fase 8: `Extrato` se há saldo. Se não há saldo, a origem é `SemDados` ("sem cotação"), e não mais `SemFonte`.
  - Com `d` antes da `data_aplicacao`, o valor é `SemDados`, como nas outras classes.
- **Fundo sem CNPJ, previdência e cripto:** nada muda (fase 8).
- **Atualização: fonte `cvm`, a sexta, depois de `b3`.**
  - Só roda se a carteira tem algum fundo com CNPJ válido. Sem nenhum, nada é baixado e o status da fonte é apagado (ela volta a "nunca atualizada"), para não pedir atualização nem mostrar erro antigo depois que o último CNPJ sai da carteira.
  - Meses que cada fundo precisa:
    - o mês da `data_aplicacao`;
    - o mês da `saldo_data`, se existir;
    - todos os meses de `max(mês da aplicação, mês de hoje − 13)` até o mês de hoje, que é o histórico de 12 fins de mês do painel.
    - Ficam de fora os meses antes de 2021-01 e os depois de hoje.
  - Marca por CNPJ e mês em `cobertura_serie`, chave `cvm:<cnpj>:<AAAA-MM>`, valor = data do download (`gravar_marca_recente`).
  - Um mês é baixado de novo para um CNPJ quando não tem marca, ou quando a marca é anterior ao último dia do mês + 10 dias. Nunca mais de uma vez no mesmo dia.
  - Cada mês é baixado uma vez por atualização e filtrado pelo conjunto de CNPJs que precisam dele.
  - Data base sem cota: se não há cota ≤ a data base da aplicação ou do saldo (por exemplo, aplicação no dia 1º ou num fim de semana), o mês anterior também é processado, com as mesmas regras de marca.
  - Respostas:
    - 404 de um mês: o mês está indisponível. A marca é gravada e não é erro.
    - Falha de rede ou de formato: depois da nova tentativa padrão, a fonte falha com "A CVM não respondeu. Tente de novo mais tarde." e as outras fontes não são afetadas.
  - Status: `ultima_data` = maior data em `cota_fundo`.
- **Rodapé "Dados de mercado de dd/mm":** ignora a `cvm`, como já ignora o IPCA, porque a cota sai com dias de atraso e puxaria a data para trás.
- **Rótulos da fonte:** "Cotas de fundos (CVM)" no node; "CVM (cotas de fundos)" no contexto da IA.
- **Assistente.** `markdown.js` passa a entender:
  - tabela GFM: linha de cabeçalho com `|`, separador `|---|:---:|---:|` e linhas, com alinhamento;
  - itálico em `*x*` e `_x_`, com `_` só entre limites de palavra;
  - código inline com crase, conteúdo literal;
  - bloco de código cercado por três crases;
  - linha divisória (`---`, `***` ou `___` sozinhos);
  - citação (`> `);
  - títulos de `#` a `######`;
  - link `[texto](url)`, que vira só o texto, sem âncora e sem abrir nada.

  Nada vira HTML: os elementos são criados com `textContent`. A tabela fica numa caixa com rolagem horizontal dentro da bolha. Durante o streaming, uma tabela ainda sem a linha separadora aparece como parágrafo e vira tabela quando a linha chega.
- **Textos:** o aviso de fontes (`avisos.js`/`docs/avisos.md`) passa a dizer de onde vem o valor de fundos. Pela regra de cópia do projeto, frase nova em português, com acentos.

### Não-objetivos
- Zips anuais anteriores a 2021.
- Número de cotas informado pelo usuário.
- IR, come-cotas e taxas: o valor continua bruto.
- Previdência pela CVM.
- Links clicáveis nas respostas da IA.

## Global Constraints

- Identificadores, mensagens e textos em português com acentos; dinheiro em `i64` de centavos e cotas em `rust_decimal::Decimal` (nunca `f64`).
- Nenhuma dependência nova em nenhum `Cargo.toml` nem `package.json`.
- A rede só alcança URLs que `rede::classificar` reconhece. A nova URL da CVM entra na lista e o resto continua recusado.
- O valor é bruto, antes do IR; nada muda nas regras de renda fixa, Tesouro, B3, previdência e cripto.
- Verde ao fim de cada task, nas suítes que ela toca:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - em `crates/guru-node`: `npm test`
  - em `app`: `npm test` (unidade) e `npx playwright test` (e2e)
  - A falha conhecida do smoke por causa do sandbox SUID do Electron é pré-existente e pode continuar.
- Commits pequenos, mensagem em português, terminando com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Um CNPJ com só linhas de subclasse no CSV não pode pegar a cota de uma subclasse; ele fica sem cota. Teste na Task 1.
2. Aplicação num fim de semana ou no dia 1º: a cota base é a última antes da data, e o atualizador busca o mês anterior quando precisa. Testes nas Tasks 3 e 4.
3. CVM fora do ar na primeira atualização: as outras fontes atualizam, a falha aparece só na "CVM" e os fundos ficam com o valor anterior (`Extrato` ou `SemDados`). Teste na Task 3.
4. O usuário troca o CNPJ de um fundo: a próxima atualização baixa os meses do CNPJ novo mesmo com os meses já marcados para o antigo, porque as marcas são por CNPJ. Teste na Task 3.
5. O rodapé "Dados de mercado de dd/mm" não recua por causa do atraso da CVM. Teste na Task 2.

---

### Task 1: Leitor do Informe Diário, URL da CVM e CNPJ

**Files:**
- Create: `crates/guru-core/src/mercado/cvm.rs`, `crates/guru-core/src/investimento/cnpj.rs`
- Modify: `crates/guru-core/src/mercado/mod.rs`, `crates/guru-core/src/mercado/rede.rs`, `crates/guru-core/src/investimento/mod.rs`, `crates/guru-core/src/lib.rs` (reexports, se o padrão do crate pedir)
- Test: testes `#[cfg(test)]` nos próprios arquivos, montando o zip em memória com `zip::ZipWriter`, como em `importacao/xlsx.rs`

**Interfaces:**
- Produces:
  - `pub struct CotaFundo { pub cnpj: String /* 14 dígitos */, pub data: NaiveDate, pub cota: Decimal }` (derive Debug, Clone, PartialEq, Eq)
  - `pub fn ler_inf_diario_csv(texto: &str, cnpjs: &HashSet<String>) -> Result<Vec<CotaFundo>, ErroFonte>`
  - `pub fn ler_inf_diario_zip(bytes: &[u8], cnpjs: &HashSet<String>) -> Result<Vec<CotaFundo>, ErroFonte>`: lê o primeiro `.csv` do zip; o conteúdo é decodificado como latin-1 se não for UTF-8 válido
  - `pub(crate) fn url_cvm(ano: i32, mes: u32) -> String`; `Destino::Cvm { mes: String /* "AAAAMM" */ }`; `BaixadorPasta` usa o nome `inf_diario_fi_AAAAMM.zip`
  - `pub fn normalizar_cnpj(texto: &str) -> Option<String>`: tira tudo que não é dígito; `Some` só com 14 dígitos e DV válidos, e não todos iguais
  - `pub fn formatar_cnpj(digitos: &str) -> String`: devolve `00.000.000/0000-00`
  - Todos ficam exportados por `guru_core` (o node e a Task 5 usam o CNPJ)

- [ ] **Step 1: Write the failing tests**
  - `le_layout_novo_e_antigo`: um CSV com o cabeçalho novo e outro com o antigo, cada um com uma linha `11.222.333/0001-81` em `2026-11-05`, `VL_QUOTA` `1.498960195115`. O resultado é `CotaFundo{cnpj:"11222333000181", data:2026-11-05, cota: dec!(1.498960195115)}`.
  - `ignora_subclasse_e_cnpj_fora_do_filtro`: a linha com `ID_SUBCLASSE=ABC1` some, e um CNPJ fora do conjunto some. Um CNPJ que só tem subclasse devolve vazio. (Review Focus 1)
  - `recusa_csv_sem_colunas`: um cabeçalho sem `VL_QUOTA` dá `Err(ErroFonte::Formato(_))`; `VL_QUOTA` ≤ 0 ou ilegível descarta só a linha.
  - `le_do_zip`: monta um zip com `inf_diario_fi_202611.csv` e lê pelo `ler_inf_diario_zip`. Um zip inválido dá `Err(Formato)`.
  - Em `rede.rs`:
    - `url_cvm(2026, 11) == "https://dados.cvm.gov.br/dados/FI/DOC/INF_DIARIO/DADOS/inf_diario_fi_202611.zip"`, e `classificar` dá `Some(Destino::Cvm{mes:"202611"})`.
    - Entram na lista de recusa: `inf_diario_fi_202613.zip`, `inf_diario_fi_2026.zip`, `.../HIST/inf_diario_fi_2020.zip`, um sufixo `?x=1` e `http://`.
  - Em `cnpj.rs`:
    - `normalizar_cnpj("11.222.333/0001-81") == Some("11222333000181")`;
    - DV errado (`11.222.333/0001-82`), 13 dígitos e `00000000000000` dão `None`;
    - `formatar_cnpj("11222333000181") == "11.222.333/0001-81"`.
- [ ] **Step 2: Run them to see them fail** — `cargo test -p guru-core cvm cnpj rede` (erro de compilação/ausência).
- [ ] **Step 3: Implement** — colunas achadas pelo nome (`CNPJ_FUNDO_CLASSE` ou `CNPJ_FUNDO`, `DT_COMPTC`, `VL_QUOTA`, `ID_SUBCLASSE` opcional); par `(cnpj, data)` repetido fica com a primeira linha.
- [ ] **Step 4: Run them to see them pass** — mesmo comando, mais `cargo clippy -p guru-core --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** — `Mercado: leitor do Informe Diário da CVM e CNPJ de fundo`.

### Task 2: Tabela de cotas e a fonte `cvm`

**Files:**
- Create: `crates/guru-core/src/migrations/0009_cvm.sql`
- Modify:
  - `crates/guru-core/src/db.rs` (migração 9)
  - `crates/guru-core/src/mercado/repositorio.rs`: `FONTES` com 6 entradas, `"cvm"` por último
  - `crates/guru-node/src/mercado.rs`: rótulo `"cvm" => "Cotas de fundos (CVM)"`
  - `crates/guru-core/src/ia/contexto.rs:282`: `"cvm" => "CVM (cotas de fundos)"`
  - `app/renderer/js/mercado.js` (`menorData`)
  - os testes que contam 5 fontes ou citam a versão do esquema
- Test: `crates/guru-core/tests/mercado.rs`, `app/test/unit/mercado.test.mjs`

**Interfaces:**
- Consumes: `CotaFundo` (Task 1).
- Produces:
  - `cota_fundo(cnpj TEXT NOT NULL, data TEXT NOT NULL, cota TEXT NOT NULL, PRIMARY KEY (cnpj, data))`
  - `Db::gravar_cotas_fundo(&self, cotas: &[CotaFundo]) -> Result<usize, CoreError>`: upsert que devolve quantas cotas eram novas
  - `Db::cota_fundo_em(&self, cnpj: &str, data: NaiveDate) -> Result<Option<(NaiveDate, Decimal)>, CoreError>`: a última cota com data ≤ `data`
  - `Db::ultima_data_cota_fundo(&self) -> Result<Option<NaiveDate>, CoreError>`

- [ ] **Step 1: Write the failing tests**
  - `cotas_de_fundo_upsert_e_busca`: grava três cotas, em 03/11, 04/11 e 05/11, e o retorno é 3. Gravar 05/11 de novo com outro valor retorna 0 e atualiza a cota. `cota_fundo_em(c, 2026-11-08)` devolve a de 05/11, `cota_fundo_em(c, 2026-11-02)` devolve `None`, e `ultima_data_cota_fundo` é 05/11.
  - `status_fontes_tem_seis_na_ordem`: a ordem é cdi, selic, ipca, tesouro, b3, cvm.
  - Em JS, `menorData ignora cvm`: com `[{fonte:"cdi", ultimaData:"2026-11-06"}, {fonte:"cvm", ultimaData:"2026-11-04"}]`, o resultado é `"2026-11-06"`. (Review Focus 5)
- [ ] **Step 2: Run them to see them fail** — `cargo test -p guru-core --test mercado`; `cd app && npm test`.
- [ ] **Step 3: Implement** — a cota é gravada como texto `Decimal::normalize`, como `dec()` no repositório; `menorData` filtra `ipca` e `cvm`.
- [ ] **Step 4: Run them to see them pass** — `cargo test --workspace`; `cd crates/guru-node && npm test`; `cd app && npm test`.
- [ ] **Step 5: Commit** — `Mercado: tabela de cotas de fundos e a fonte CVM`.

### Task 3: Atualizador da CVM

**Files:**
- Modify: `crates/guru-core/src/mercado/atualizador.rs`
- Test: `crates/guru-core/tests/atualizador.rs`. As fixtures são zips gerados no teste numa pasta temporária, servidos pelo `BaixadorPasta`.

**Interfaces:**
- Consumes: `url_cvm`, `ler_inf_diario_zip`, `normalizar_cnpj` (Task 1); `gravar_cotas_fundo`, `cota_fundo_em`, `ultima_data_cota_fundo` (Task 2); `cobertura_desde`/`gravar_marca_recente` (existentes).
- Produces: a fonte `cvm` atualizada dentro de `atualizar(...)`, que mantém a mesma assinatura. As constantes são `MSG_CVM = "A CVM não respondeu. Tente de novo mais tarde."`, `PRIMEIRO_MES_CVM = (2021, 1)` e `FOLGA_FECHAMENTO_CVM_DIAS = 10`.

- [ ] **Step 1: Write the failing tests** (`hoje = 2026-11-07`)
  - `cvm_baixa_os_meses_do_fundo`:
    - O fundo tem CNPJ válido, aplicação em 2026-09-08 e fixtures 202609, 202610 e 202611.
    - Depois de `atualizar`, há cotas dos três meses só desse CNPJ (o CSV traz outro CNPJ que não pode entrar).
    - O status `cvm` não tem erro, e `ultima_data` é a maior data.
  - `cvm_nao_roda_sem_fundo_com_cnpj`: um fundo sem CNPJ não faz nenhum pedido à CVM (o `Baixador` de teste conta as URLs) e o status `cvm` fica com tudo `None`.
  - `cvm_mes_fechado_nao_e_baixado_de_novo`: numa segunda chamada no dia seguinte, 202609 e 202610 não são pedidos de novo, porque as marcas são posteriores a fim do mês + 10 dias. 202611 é pedido de novo.
  - `cvm_mesmo_dia_nao_repete`: duas chamadas no mesmo `hoje` fazem os pedidos uma vez só.
  - `cvm_cnpj_novo_baixa_meses_ja_marcados` (Review Focus 4): depois da primeira atualização, troca o `codigo_b3` do fundo por outro CNPJ válido presente nas fixtures. A segunda atualização, no dia seguinte, grava as cotas do CNPJ novo também em 202609.
  - `cvm_aplicacao_sem_cota_busca_mes_anterior` (Review Focus 2): com aplicação em 2026-10-01 e a primeira cota de 202610 em 2026-10-02, o atualizador pede 202609 e grava a cota de 2026-09-30.
  - `cvm_404_nao_e_erro`: sem o arquivo de 202611, o status fica sem erro e as cotas dos outros meses estão gravadas.
  - `cvm_fora_do_ar_nao_derruba_as_outras` (Review Focus 3): o `Baixador` devolve `Rede` para toda URL da CVM e serve as outras fixtures. O status `cvm` traz `MSG_CVM`; `cdi`, `selic` e `b3` ficam sem erro.
  - `cvm_ignora_meses_antes_de_2021`: com aplicação em 2019-05-10, nenhum pedido sai antes de 202101 (a janela de 13 meses vale para o resto).
- [ ] **Step 2: Run them to see them fail** — `cargo test -p guru-core --test atualizador cvm`.
- [ ] **Step 3: Implement** — um método `Contexto::cvm(&self, fundos: &[(String /*cnpj*/, NaiveDate /*aplicação*/, Option<NaiveDate> /*saldo_data*/)]) -> Result<usize, Falha>`, fechado com `fechar("cvm", ...)`. A rede usa `com_nova_tentativa`, mas o 404 (`ErroFonte::Status(404)`) é resposta definitiva, sem repetir.
- [ ] **Step 4: Run them to see them pass** — `cargo test --workspace`, clippy.
- [ ] **Step 5: Commit** — `Mercado: atualização das cotas de fundos pela CVM`.

### Task 4: Valorização do fundo pela cota

**Files:**
- Modify: `crates/guru-core/src/mercado/valorizacao.rs`, mais os pontos que fazem `match` exaustivo em `Origem` (procure `Origem::Extrato` em `crates/`)
- Test: `crates/guru-core/tests/valorizacao.rs`

**Interfaces:**
- Consumes: `cota_fundo_em` (Task 2), `normalizar_cnpj` (Task 1).
- Produces: `Origem::Cota`, com `codigo() == "cota"` e `rotulo() == "cota diária da CVM"`. No renderer, `celulasValor` já trata qualquer origem fora de `sem_fonte`/`sem_dados` como valor com data, então não muda.

- [ ] **Step 1: Write the failing tests** (cotas gravadas direto com `gravar_cotas_fundo`)
  - `fundo_manual_pela_cota`:
    - O fundo tem aplicado 30000, aplicação em 2026-09-08 e cotas 1,40 em 09-08 e 1,4989 em 11-05.
    - Valorizado em 2026-11-07, o valor é `floor(30000 × 1.4989 / 1.40) = 32119`, com origem `Cota` e referência 2026-11-05.
  - `fundo_aplicado_no_fim_de_semana_usa_cota_anterior` (Review Focus 2): com aplicação em 2026-09-06 (domingo), a cota base é a de 09-04.
  - `fundo_com_saldo_projeta_o_saldo`:
    - O fundo tem saldo 50000 em 2026-10-31, cota 2,00 em 10-30 e cota 2,02 em 11-05.
    - O valor é 50500, com origem `Cota`.
    - Valorizado em 2026-10-31, mesma data da cota base, o valor é 50000 com origem `Extrato`.
  - `fundo_com_cnpj_sem_cotas`: sem saldo, a origem é `SemDados` e o valor é o aplicado. Com saldo, vale o `Extrato` (a regra da fase 8 continua).
  - `fundo_sem_cnpj_continua_sem_fonte` e `prev_e_cripto_nao_mudam`.
  - `fundo_antes_da_aplicacao_sem_dados`.
- [ ] **Step 2: Run them to see them fail** — `cargo test -p guru-core --test valorizacao fundo`.
- [ ] **Step 3: Implement** — um `fn fundo_pela_cota(db, inv, data) -> Result<Option<Valor>, CoreError>` chamado no ramo `fundo` de `valorizar_com`, antes da regra do extrato. A conta é em `Decimal` com `checked_mul`/`checked_div`; um estouro dá `None` e cai na regra antiga.
- [ ] **Step 4: Run them to see them pass** — `cargo test --workspace`, clippy; `cd crates/guru-node && npm test`.
- [ ] **Step 5: Commit** — `Valorização: fundo com CNPJ pela cota diária da CVM`.

### Task 5: CNPJ no cadastro do fundo

**Files:**
- Modify:
  - `crates/guru-core/src/investimento/{modelo.rs,validacao.rs,repositorio.rs}`
  - `crates/guru-node/src/investimentos.rs`
  - `app/renderer/js/telas/formulario-investimento/dados.js`
  - `app/renderer/js/telas/formulario-investimento.js`
  - a lista de investimentos, se ela mostrar o código
- Test: `crates/guru-core/src/investimento/validacao.rs` (testes do módulo), `crates/guru-core/tests/investimentos.rs`, `app/test/unit/formulario-dados.test.mjs`, um caso em `app/test/e2e/` no spec do formulário existente

**Interfaces:**
- Consumes: `normalizar_cnpj`, `formatar_cnpj` (Task 1).
- Produces:
  - `NovoInvestimento.cnpj: Option<String>` e `InvestimentoValido.cnpj: Option<String>` (14 dígitos), só para o tipo `fundo`.
  - No repositório, a inserção grava `codigo_b3 = cnpj` quando o tipo é `fundo`. A edição de um fundo grava `codigo_b3 = cnpj`: `NULL` apaga, e a edição de outros tipos não toca em `codigo_b3`.
  - No JS, o campo de entrada é `cnpj` e a saída `InvestimentoJs.cnpj` vem formatada `00.000.000/0000-00`, ou `null` se o tipo não é fundo ou se o código não é um CNPJ válido.

- [ ] **Step 1: Write the failing tests**
  - Validação de fundo:
    - `cnpj` vazio ou ausente → `None`, sem erro;
    - `"11.222.333/0001-81"` → `Some("11222333000181")`;
    - `"123"` → erro no campo `cnpj` com a mensagem `"CNPJ inválido. Confira os 14 dígitos."`;
    - um CNPJ em tipo `rf-cdb` é ignorado (`None`).
  - Repositório:
    - criar um fundo com CNPJ grava o `codigo_b3`;
    - editar sem CNPJ apaga;
    - editar um `rv-acao` com `codigo_b3` preexistente o mantém.
  - Em `formulario-dados.test.mjs`, o tipo fundo inclui `cnpj` em `dados`, e outros tipos não.
  - No e2e, cadastra um "Fundo de investimento" com CNPJ, reabre a edição e vê o CNPJ formatado.
- [ ] **Step 2: Run them to see them fail** — `cargo test -p guru-core investimento`; `cd app && npm test`.
- [ ] **Step 3: Implement**
  - O campo "CNPJ do fundo (opcional)" aparece só para o tipo fundo, com a dica "Com o CNPJ, o valor atual vem da cota diária publicada pela CVM."
  - O salvamento já dispara a atualização de mercado em segundo plano (`formulario-investimento.js:313`), e isso basta.
- [ ] **Step 4: Run them to see them pass** — `cargo test --workspace`; `cd crates/guru-node && npm test`; `cd app && npm test && npx playwright test`.
- [ ] **Step 5: Commit** — `Investimentos: CNPJ do fundo no cadastro`.

### Task 6: Markdown completo nas respostas da IA

**Files:**
- Modify: `app/renderer/js/markdown.js`, `app/renderer/css/telas.css` (bloco `.bolha.ia`)
- Test: `app/test/unit/markdown.test.mjs`. Todos os testes atuais continuam passando sem mudança.

**Interfaces:**
- Produces, como blocos novos de `analisarMarkdown`:
  - `{tipo:"titulo", nivel:1..6, trechos}`, em que `nivel` é novo e os testes antigos de título continuam válidos;
  - `{tipo:"tabela", alinhamentos:("esquerda"|"centro"|"direita"|null)[], cabecalho: trechos[], linhas: trechos[][]}`;
  - `{tipo:"codigo", texto}`, `{tipo:"hr"}` e `{tipo:"citacao", trechos}`.
- Produces, como trechos novos: `{tipo:"italico", valor}` e `{tipo:"codigo", valor}`. O link vira um trecho `texto` com o texto do link.
- O render usa:
  - `<div class="md-tabela"><table><thead>…</thead><tbody>…</tbody></table></div>`, com `style.textAlign` por coluna;
  - `<pre><code>`, `<hr>` e `<blockquote>`;
  - `<em>` e `<code>`;
  - para o título, um `<p><strong>` como hoje.

- [ ] **Step 1: Write the failing tests**
  - `tabela simples com alinhamento`: o texto `"| Ativo | Valor |\n|:---|---:|\n| CDB | **R$ 10** |"` vira um bloco tabela com `alinhamentos ["esquerda","direita"]`, o cabeçalho `[[texto Ativo],[texto Valor]]` e a linha 1, coluna 2, igual a `[negrito "R$ 10"]`.
  - `linha de tabela sem separador fica parágrafo`, que é o caso do streaming.
  - `tabela com células a menos ou a mais`: as que faltam viram vazias e as que sobram são descartadas.
  - `pipe escapado na célula`: `a \| b` fica numa célula só, com o texto `a | b`.
  - `tabela sem pipes nas bordas`: `Ativo | Valor\n--- | ---\nCDB | 10` também é tabela.
  - `itálico com asterisco e sublinhado`; `nome_de_variavel` não vira itálico.
  - `código inline é literal`: `` `**x**` `` vira o trecho código `**x**`.
  - `bloco de código cercado preserva linhas e indentação`.
  - `linha divisória`, `citação` e `título de nível 4 a 6`.
  - `link vira só o texto`: `[site](https://x.com)` vira o texto `site`, e o marcador de fonte `[1]` continua funcionando ao lado.
  - `<script> em célula de tabela é texto literal`.
- [ ] **Step 2: Run them to see them fail** — `cd app && node --test test/unit/markdown.test.mjs`.
- [ ] **Step 3: Implement**
  - Dentro de um bloco cercado não há parsing; um bloco aberto sem fechar vai até o fim do texto.
  - Detecção de tabela: a linha tem `|` e a próxima linha é um separador válido com o mesmo número de colunas.
  - CSS:
    - `.bolha.ia .md-tabela { overflow-x: auto; max-width: 100%; }`;
    - a tabela com `border-collapse`, bordas `var(--line)`, padding 4px 8px, números com `font-variant-numeric: tabular-nums` e `overflow-wrap: normal; white-space: nowrap` nas células de cabeçalho;
    - `pre` com rolagem horizontal e `white-space: pre`;
    - `blockquote` com borda esquerda.
- [ ] **Step 4: Run them to see them pass** — `cd app && npm test && npx playwright test` (os e2e do assistente continuam verdes).
- [ ] **Step 5: Commit** — `Assistente: tabelas, itálico, código e citação nas respostas`.

### Task 7: Textos, documentação e verificação final

**Files:**
- Modify:
  - `app/renderer/js/avisos.js:12` e `docs/avisos.md:17` (a mesma frase nos dois)
  - `docs/mvp.md:59`
  - `docs/arquitetura.md` (Rede: destino da CVM; Dados: `cota_fundo`)
  - `docs/roadmap.md` (linha da fase 9 e link deste plano)
  - `docs/pendencias.md` (marcar os dois itens)
  - `app/avisos-terceiros.txt` (só se o teste de avisos de terceiros pedir)
- Test: `app/test/unit/avisos.test.mjs`, se ele comparar a frase.

- [ ] **Step 1:** No aviso de fontes, troque "Fundos, previdência e cripto contam pelo valor aplicado." por "Fundos com CNPJ usam a cota diária publicada pela CVM, que sai com alguns dias de atraso. Fundos sem CNPJ, previdência e cripto contam pelo saldo do último extrato importado ou, sem ele, pelo valor aplicado." Faça a troca nos dois arquivos e em `docs/mvp.md`, adaptada à frase da tabela.
- [ ] **Step 2:** Atualize a documentação listada.
- [ ] **Step 3: Verificação completa** — `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`; `cd crates/guru-node && npm test`; `cd app && npm test && npx playwright test`. O esperado é tudo verde, exceto a falha pré-existente do smoke por causa do sandbox SUID.
- [ ] **Step 4: Commit** — `Fase 9: textos e documentação`.
