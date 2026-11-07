# Arquitetura

Como o Guru de Investimentos é montado por dentro. O que o app faz está em [`mvp.md`](mvp.md). A ordem de construção está em [`roadmap.md`](roadmap.md).

## Stack

| Camada | Tecnologia |
|--------|------------|
| Janela | Electron |
| Interface | HTML, CSS e JavaScript puros, sem framework e sem etapa de build |
| Regras, dados e rede | Rust |
| Ponte entre os dois | Addon nativo Node com [napi-rs](https://napi.rs) |
| Banco local | SQLite, via `rusqlite` com a biblioteca embutida (`bundled`) |
| Chave da IA | Cofre do sistema operacional, via crate `keyring` |
| HTTP | `ureq` |
| Empacotamento e atualização | Empacotamento: electron-builder 26.17.0; atualização: electron-updater 6.8.10 |

## Camadas

```
┌──────────────────────── Electron ────────────────────────┐
│  renderer/  (HTML, CSS, JS)                              │
│     window.guru.*  ──  só o que o preload expõe          │
│        │ ipcRenderer.invoke("canal", dados)              │
│  preload.js  (contextBridge, sem lógica)                 │
│        │                                                 │
│  main.js  (janela, ciclo de vida, roteia IPC → Rust)     │
│        │ chamada de função                               │
├────────┼─────────────────────────────────────────────────┤
│  guru-node  (Rust, napi-rs: converte tipos e erros)      │
│        │                                                 │
│  guru-core  (Rust puro: domínio, SQLite, importação,     │
│              indicadores, IA)                            │
└──────────────────────────────────────────────────────────┘
```

- **`guru-core`** guarda toda regra de negócio e todo acesso a disco e rede. Não sabe que existe Electron e é testado só com `cargo test`.
- **`guru-node`** é fino. Converte tipos JS ↔ Rust, transforma erros em `Error` do JS com um `code` e roda o trabalho pesado fora da thread principal (funções `async` do napi-rs).
- **`main.js`** cria a janela, registra um handler de IPC por função exposta e repassa a chamada ao addon. Não tem regra de negócio.
- **`preload.js`** expõe `window.guru` com `contextBridge`. Cada função chama um canal fixo. A interface nunca recebe `ipcRenderer` nem nada do Node. O `main.js` responde sempre com um envelope, `{ ok: true, value }` ou `{ ok: false, code, message }`, e o preload o devolve como está, porque o Electron só deixa passar a mensagem de um erro.
- **`renderer/`** desenha as telas. Chama sempre por `renderer/js/api.js`, que transforma a falha do envelope em `Error` com `code`. Não faz requisição de rede.

### Por que addon nativo e não um processo separado

- Não abre porta local, então outro programa do computador não consegue falar com o núcleo.
- A N-API é estável entre versões do Node e do Electron, então o addon não precisa ser recompilado a cada atualização do Electron.
- O napi-rs gera os tipos TypeScript (`index.d.ts`) das funções expostas, o que documenta o contrato da ponte.
- Um `panic` no Rust vira erro JS em vez de derrubar o app.

A alternativa (binário Rust separado falando JSON por stdin/stdout) continua viável. Se o addon der problema no empacotamento, a troca fica restrita ao `guru-node` e ao `main.js`, porque o `guru-core` não muda.

## Dados

- Tudo fica na pasta de dados do usuário que o Electron informa (`app.getPath("userData")`), no arquivo `guru.db`.
- **Dinheiro em centavos, como inteiro (`i64`).** Nunca `f64` para valor em reais.
- **Taxas e percentuais em decimal exato** (`rust_decimal`), por exemplo `112.5` (% do CDI) ou `6.25` (% a.a.).
- Datas como `YYYY-MM-DD` no banco e `chrono::NaiveDate` no Rust.
- O esquema evolui por migrações numeradas dentro do `guru-core`. Um banco criado por uma versão mais nova do app não é aberto por uma mais antiga: o app avisa em vez de arriscar estragar os dados.
- A chave da IA fica no cofre do sistema (Keychain, Credential Manager, Secret Service). Nunca vai para o SQLite nem para a interface.

Dados de mercado ficam em sete tabelas, criadas pela migração `0004`:

| Tabela | Guarda |
|--------|--------|
| `indice_diario` | CDI e Selic por dia |
| `ipca_mensal` | IPCA por mês |
| `tesouro_preco` | Taxa e preço unitário de compra e base de cada título, por dia |
| `cotacao` | Preço de fechamento de cada ticker por dia, em centavos |
| `fonte_status` | Última data, última atualização e erro de cada fonte |
| `cobertura_serie` | Desde quando cada série já foi pedida, para não repetir pedidos |
| `b3_alvo` | Para cada data pedida à B3, o pregão encontrado e os tickers pedidos |

A migração `0005` acrescenta `investimentos.codigo_b3` (código do ativo na planilha: ticker na renda variável, `Código` na renda fixa, ISIN ou `<tipo>:<vencimento>` no Tesouro) e a tabela `importacao`, com a data, o nome do arquivo, o total de linhas e quantas foram aplicadas em cada importação.

## Segurança da janela

- `contextIsolation: true`, `sandbox: true`, `nodeIntegration: false`.
- Content-Security-Policy sem scripts externos: `default-src 'self'`. As fontes IBM Plex ficam empacotadas no app.
- Links externos (fontes citadas, notícias) abrem no navegador padrão pelo `shell.openExternal`, e só com `https:`.
- Uma instância por vez (`app.requestSingleInstanceLock`), para dois processos não gravarem no mesmo banco.

## Rede

Só o `guru-core` acessa a rede, e só estes destinos, com uma exceção: a procura e o download de atualizações, que ficam no processo principal (`app/main/atualizacao-electron.js`, fase 7). No `guru-core`, qualquer outra URL é recusada antes da requisição, e cada requisição tem timeout de 60 s e `User-Agent: GuruDeInvestimentos/<versão>`.

Só a Anthropic recebe dado do usuário, com a chave dele e só as partes do contexto que ele deixou ligadas. As fontes de mercado recebem só datas e códigos de série nas URLs.

| Destino | Para quê | Fase |
|---------|----------|------|
| `api.bcb.gov.br` (SGS) | Selic e CDI; IPCA pela série 433 do SGS (dado do IBGE publicado pelo Banco Central) | 3 |
| Tesouro Transparente | Taxas e preços do Tesouro Direto | 3 |
| B3, série histórica (arquivo diário COTAHIST) | Preço de fechamento de ações, FIIs e ETFs | 3 |
| `https://api.anthropic.com/v1/messages` (POST) | Perguntas ao Claude, com a chave do usuário | 6a |
| `https://api.anthropic.com/v1/messages` (POST) | Insights e notícias do painel da Carteira, só pelos botões | 6b |
| `https://api.anthropic.com/v1/models?limit=1` (GET) | Conferir a chave ao salvar, sem custo | 6a |
| `https://api.github.com/repos/guilhermebecko/Guru-de-Investimentos/releases/latest` (GET) | Procurar uma versão nova: ao abrir o app, se "Procurar atualizações ao abrir o app" estiver ligada, e pelo botão "Procurar agora". Processo principal. | 7 |
| `https://github.com/guilhermebecko/Guru-de-Investimentos/releases…` e `*.githubusercontent.com` | Baixar a versão nova pelo electron-updater (`releases.atom`, o `latest.yml` ou `latest-linux.yml` da tag e o instalador, com o redirecionamento do GitHub), só depois do clique em "Baixar e instalar", no Windows e no AppImage. Processo principal. | 7 |

Os pedidos ao GitHub não levam nenhum dado do usuário: só a versão do app no `User-Agent` (`GuruDeInvestimentos/<versão>`), além do IP. A procura leva `Accept: application/vnd.github+json`, tem prazo de 30 s e trata uma resposta acima de 1 MB como erro. Fora do app empacotado, sem `GURU_TESTE_ATUALIZACAO`, nada é consultado.

Os três pedidos à Anthropic levam os cabeçalhos `x-api-key`, `anthropic-version: 2026-10-21`, `content-type: application/json` e o mesmo `User-Agent`. Com Opus e Sonnet, os pedidos a `/v1/messages` (perguntas, insights e notícias) levam também `anthropic-beta: server-side-fallback-2026-07-01`. Prazos: 60 s para conectar e 10 min para a resposta inteira (30 s no total para conferir a chave). Uma resposta acima de 8 MB é cortada com erro. A resposta chega por streaming (SSE) e é lida numa thread do addon, com conexão própria ao banco; a tela acompanha o pedido a cada 150 ms e pode pará-lo.

Os dois pedidos do painel da Carteira (6b) usam o mesmo destino e os mesmos cabeçalhos, e só saem dos botões "Gerar insights" e "Buscar notícias":

- **Insights:** levam o mesmo contexto de uma conversa, com as partes que o usuário deixou ligadas e as mesmas chaves de privacidade, mais um pedido fixo. Usam saída estruturada (`output_config.format` com JSON Schema) e não levam a busca na web, mesmo com "Buscar na web" ligada.
- **Notícias:** levam só a lista de ativos (tickers e os tipos de renda fixa com o indexador), sem nenhum valor, nome de investimento, instituição ou emissor, mais as instruções do usuário. Com a parte "Carteira" desligada, vão só as classes presentes, sem tickers. Levam sempre a busca na web; com "Buscar na web" desligada, nada é enviado. Cada notícia guarda o link, o domínio e a data da fonte que a resposta citou, e só links `https`.

## Chave da IA

- Fica no cofre do sistema (Keychain, Credential Manager, Secret Service), com serviço `guru-de-investimentos` e usuário `anthropic`.
- O SQLite guarda só os 4 últimos caracteres (`chave_final`), para a tela mostrar `sk-ant-…4f2a`.
- O main só repassa ao addon a chave digitada. O addon não a devolve em nenhuma resposta, e ela não entra em erro, log, conversa gravada nem contexto.
- Ao salvar, o formato é conferido (`sk-ant-`, de 20 a 300 caracteres, sem espaço) e a chave é testada com `GET /v1/models?limit=1`. Só uma chave aceita pela Anthropic é gravada. "Remover chave" apaga do cofre e limpa `chave_final`.

### Modo de teste da IA

Os testes nunca chamam a API real. No núcleo, o `ClienteFixture` lê respostas gravadas (`.sse`). Nos e2e, `GURU_IA_FIXTURES=<pasta>` troca a rede por essa pasta e `GURU_IA_COFRE=memoria` troca o cofre do sistema por um em memória. Uma marca na pergunta escolhe o arquivo (`#busca`, `#erro401`, `#recusa`...). Os insights e as notícias não têm pergunta: a marca vai nas instruções do usuário, que chegam no contexto (`#insights-invalido`, `#noticias-vazias`, `#noticias-recusa`, `#noticias-max-tokens`, `#noticias-pausa`, e `#erro401` e `#erro529` também valem para eles). As duas variáveis valem só fora do app empacotado.

## Atualização de mercado

O atualizador roda no addon, em uma thread de fundo com a própria conexão ao banco. Os downloads não travam a tela.

- Busca só o que falta. Uma aplicação antiga cadastrada depois faz o app buscar para trás até a data dela.
- Da B3 guarda o último pregão e o último pregão de cada um dos 12 meses anteriores.
- Cada fonte falha sozinha. A que falhou mostra a mensagem e mantém os dados que já tinha; as outras seguem.
- Uma nova tentativa depois de 5 s. Na B3 só erro de rede é repetido, porque arquivo que não existe significa dia sem pregão.
- Resposta HTML ou XML de erro do SGS, com status 200, conta como falha da fonte.

O app atualiza ao abrir, se alguma fonte não foi atualizada hoje, e de novo depois de salvar um investimento ou de aplicar uma importação (da B3 ou da BTG). O botão "Atualizar" da barra lateral faz a mesma coisa sob demanda.

## Importação da B3

A planilha de posição é lida e comparada com a carteira no `guru-core`. Ela nunca sai do computador: a rede não é usada nessa etapa.

- O diálogo de arquivo fica no processo principal (`main.js`). Antes de ler, o tamanho do arquivo é conferido, e o caminho nunca passa para o addon.
- O addon recebe só os bytes e o nome do arquivo, lê o XLSX (`importacao/xlsx.rs`, sobre `zip` e `quick-xml`) e guarda a última comparação sob um token. O token é consumido ao aplicar, então a mesma comparação não é gravada duas vezes.
- Uma comparação nova descarta a anterior logo no início: se ela falhar, o token antigo também deixa de valer (`IMPORTACAO_EXPIRADA`).
- `codigo_b3` decide o casamento. Na renda fixa, a linha casa primeiro pelo código e só depois por emissor, vencimento e tipo; com dois ou mais candidatos, nenhum é consumido e cada linha oferece "Vincular" para todos. Na renda variável e no Tesouro, ele decide entre Atualizar (o investimento já veio de uma importação, então o valor da B3 é a atualização padrão) e Divergente (cadastrado à mão, então o padrão é manter o do usuário).
- Aplicar roda em uma única transação `IMMEDIATE`: tudo ou nada, e espera a atualização de mercado terminar de gravar em vez de falhar com o banco ocupado. O mesmo investimento vinculado a duas linhas, ou vinculado e também confirmado por outro item, é `DECISAO_INVALIDA`.
- Limites contra arquivo malicioso: 20 MB no arquivo; 50 MB descompactados por parte do ZIP; 50 abas; 100 mil linhas; 256 colunas; 2 milhões de células; 32.767 caracteres de texto por célula; 64 caracteres por número; 1 milhão de textos compartilhados; 200 relações entre partes; 100 MB de texto. Acima disso, ou se não for XLSX, o erro é `ARQUIVO_INVALIDO`.
- Valor em reais que não cabe em centavos vira o aviso "Valor fora do limite em <produto>." e a linha fica fora da comparação. Um pânico no núcleo durante a comparação ou a aplicação vira o erro `ERRO_INTERNO` em vez de derrubar o app.
- Na barra lateral, o rodapé mostra "Última importação: dd/mm/aaaa".

## Importação da BTG

O extrato da Conta Investimento da BTG (PDF) segue o mesmo caminho da B3: diálogo no `main.js`, bytes para o addon, comparação guardada sob um token e aplicação na mesma transação. O PDF também não sai do computador.

- O texto vem do `pdf-extract` (Rust puro, sem programa instalado na máquina). `importacao/pdf.rs` agrupa os caracteres posicionados em linhas e células; `importacao/btg/` reconhece as seções e lê as tabelas.
- O PDF da BTG perde glifos dos rótulos: as fontes dos títulos e cabeçalhos mapeiam uma variante do `a` e as ligaduras `fi`/`fl` para U+0000, e o `pdf-extract` as entrega como `\0`; o emissor tirado de um título troca o `\0` por `a`. Por isso os rótulos são comparados pelo "esqueleto" (`esqueleto`: sem `\0`, sem `a`, sem `fi`/`fl`, sem maiúsculas, espaços nem pontuação), aplicado aos dois lados da comparação; funciona com o PDF defeituoso e com um corrigido.
- Limites: 20 MB no arquivo (conferido no `main.js`) e 200 páginas. Acima disso, ou se não for PDF, ou se for PDF que não é um extrato da BTG, o erro é `ARQUIVO_INVALIDO`. Seção que o leitor não reconhece, e saldo fora do limite ou de fundo sem data de referência, viram aviso, não importação.
- A migração `0008` acrescenta `investimentos.saldo_centavos` e `saldo_data`: o saldo bruto de fundos e cripto, com a data a que ele se refere. A valorização usa esse saldo quando existe (origem `Origem::Extrato`, com a data visível) e cai no valor aplicado quando não há. A importação seguinte substitui o saldo.
- Um investimento por ativo e por corretora, nas duas importações. A corretora é comparada por `chave_instituicao`: o nome normalizado, ligado ao nome do catálogo quando contém a marca dele (`BTG PACTUAL CTVM S/A` e `BTG Pactual` são a mesma). Tudo o que vem do extrato é da `BTG Pactual`.
- Sem par na mesma corretora, o mesmo ativo em outra corretora vira candidato ("divergente", com `Vincular` e o padrão Ignorar) só quando a corretora dele foi digitada à mão (fora do catálogo) ou quando o código gravado é o mesmo; numa corretora do catálogo, sem código, a linha é nova. Um investimento de corretora digitada à mão com o código gravado igual ao da linha (depois de um `Vincular`) conta como par da mesma corretora: igual ou atualizar, sem mudar a corretora dele. Fundo ou cripto da BTG lançado à mão com nome parecido (um nome contém o outro em palavras inteiras) também vira candidato a `Vincular`.

### Modo de teste da importação

Nos e2e, `GURU_TESTE_ARQUIVO_B3=<caminho>` (ou `GURU_TESTE_ARQUIVO_BTG=<caminho>`, para o PDF) faz o diálogo devolver esse arquivo, e `=cancelar` simula o cancelamento. Valem só fora do app empacotado.

## Repositório

```
Guru-de-Investimentos/
├── Cargo.toml              workspace Rust
├── crates/
│   ├── guru-core/          domínio, banco, rede
│   └── guru-node/          addon napi-rs
├── app/
│   ├── package.json        Electron e scripts
│   ├── main.js
│   ├── preload.js
│   ├── renderer/
│   │   ├── index.html
│   │   ├── css/            tokens, base, telas
│   │   ├── fonts/          IBM Plex empacotada
│   │   └── js/             roteador e uma pasta por tela
│   └── test/               testes node:test e e2e
└── docs/
```

## Testes

| O quê | Como |
|-------|------|
| `guru-core` | `cargo test` |
| Ponte `guru-node` | `node --test` carregando o addon compilado |
| JS puro da interface (roteador, formatação) | `node --test` |
| App inteiro | Playwright com `_electron.launch` |

A CI (GitHub Actions) roda fmt, clippy e os testes do núcleo Rust só em Linux; o addon (build e testes) e o app (testes unitários e E2E) rodam em Linux, Windows e macOS.

O pacote também é provado na CI. Em todo PR, nos três sistemas, `npm run dist:dir` empacota sem instalador e o smoke (`--guru-smoke`) roda na pasta desempacotada; o passo falha sem `GURU_SMOKE_OK` ou com código diferente de 0. No Linux, a CI confere também os avisos de terceiros (`npm run avisos:conferir`). No push para a `main`, a CI gera os instaladores completos e os guarda como artefatos por 7 dias. O `app/test/unit/workflows.test.js` confere nos `.yml` que nenhum passo publica.

### Modo de teste de mercado

Os testes do núcleo nunca acessam a rede: usam a pasta `crates/guru-core/tests/fixtures/` (dados reais de 28/10/2026) e um `Baixador` falso.

Nos e2e, o app lê os arquivos de uma pasta em vez de baixar, com duas variáveis:

- `GURU_MERCADO_FIXTURES=<pasta>`: pasta com `sgs-<serie>.json`, `tesouro.csv` e `COTAHIST_D<ddMMaaaa>.ZIP`.
- `GURU_MERCADO_HOJE=AAAA-MM-DD`: data que substitui "hoje" só nos dados de mercado (atualização e valorização). Os e2e usam `2026-10-28`.

As duas valem só fora do app empacotado.

## Distribuição e atualização

O electron-builder empacota o app com a configuração de `app/electron-builder.yml`, sempre com `--publish never`.

- **Formatos e arquiteturas:** Windows x64 (NSIS), macOS arm64 e macOS x64 em dois `.dmg` separados, e Linux x64 (AppImage e `.deb`). A configuração não lista arquiteturas: cada runner constrói a própria, e o workflow passa `--x64` ou `--arm64`. Não há DMG universal.
- **Addon:** o addon napi é construído no runner de cada sistema e arquitetura e entra no pacote pelo `node_modules`. O `.node` fica fora do asar (`asarUnpack`), e o código Rust e os testes do addon ficam fora do pacote.
- **Fusíveis do Electron:** `RunAsNode`, `NODE_OPTIONS` e os argumentos de inspeção desligados, e `OnlyLoadAppFromAsar` ligado. A integridade do asar fica de fora, porque no macOS ela depende de assinatura.
- **Assinatura pendente:** o projeto não tem certificado. O Windows sai sem assinatura, e o macOS com a assinatura ad hoc e sem notarização. Os workflows passam os segredos de assinatura só quando eles existem; ligar a assinatura depois é cadastrar os segredos e trocar `notarize: false` por `true`.
- **Smoke do pacote:** `--guru-smoke` abre o núcleo numa pasta temporária com o cofre em memória, confere o conteúdo do pacote, imprime `GURU_SMOKE_OK` e sai com 0, sem janela e sem rede.
- **Release:** uma tag `v*` dispara o `.github/workflows/release.yml`, que confere que a tag é `v` + a versão do `app/package.json`, constrói os instaladores em `ubuntu-latest`, `windows-latest`, `macos-latest` (arm64) e `macos-15-intel` (x64), roda o smoke em cada um e cria a release em rascunho, com o `SHA256SUMS.txt`. Quem publica é uma pessoa.
- **Atualização:** a procura é do processo principal e consulta a última release publicada. Só no Windows (NSIS) e no AppImage o electron-updater baixa e instala, e só pelos cliques em "Baixar e instalar" e "Reiniciar e atualizar". No macOS e no `.deb`, o app mostra a versão nova e o link "Ver no GitHub".
- **`atualizacao.json`:** na pasta de dados, guarda `{ "procurarAoAbrir": true|false }`, o ajuste "Procurar atualizações ao abrir o app". É assunto da casca Electron, não do banco.

### Modo de teste da atualização

Nos e2e, `GURU_TESTE_ATUALIZACAO=<arquivo.json>` troca a API do GitHub por esse arquivo, no formato `{ "formato": "nsis"|"appimage"|"dmg"|"deb", "ultima": { "tag", "html_url" } | null, "erro": true|false }`. O instalador falso avança o progresso em 0, 50 e 100 e, em "Reiniciar e atualizar", só muda o estado, sem fechar o app. A variável vale só fora do app empacotado.

## Em aberto

