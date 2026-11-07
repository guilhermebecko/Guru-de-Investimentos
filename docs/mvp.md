# MVP do Guru de Investimentos

Escopo combinado durante a fase de protótipo. O protótipo das telas está em [`prototipo/index.html`](prototipo/index.html).

## Telas

| # | Tela | O que faz |
|---|------|-----------|
| 1 | Perfil de investidor | Questionário pelo método ANBIMA. Nenhum sinal do resultado durante o teste. Perguntas de consistência e resultado só no fim, com alocação-alvo ajustável. |
| 2 | Objetivo e metas | Objetivo em texto livre ("por que você investe, o que é sucesso"), lido pela IA em toda análise. Metas com valor, prazo, aporte mensal necessário e alerta de atraso. |
| 3 | Importar da B3 | Lê a planilha da Área do Investidor e compara com o que já está cadastrado: novo, atualizar, divergente, igual. O usuário escolhe o que fazer em cada linha. O que não veio na planilha nunca é apagado sozinho. |
| 4 | Lançar manualmente | Campos fechados: tipo em lista, indexador, rendimento em dois campos (% + benchmark), datas por calendário, valor em reais. Preenche IR e garantia conforme o tipo. |
| 5 | Carteira | Painel principal: saudação diferente a cada abertura, KPIs, evolução do patrimônio × total aportado, pizza por classe × alvo, rentabilidade × CDI/IPCA/BOVA11, mês a mês, instituições, vencimentos, altas e quedas. Insights da IA e notícias com link. |
| 6 | Detalhe da classe | Investimentos da classe e gráfico contra um benchmark à escolha (CDI, Selic, IPCA, BOVA11, XFIX11). |
| 7 | Próximo aporte | Divisão do valor por classe por rebalanceamento (cálculo sem IA). Cada classe abre um chat com a IA focado em escolher o investimento dentro dela. |
| 8 | Assistente | Chat com contexto da carteira, perfil, objetivo e mercado. Citações numeradas e painel com cada fonte, link e data. |
| 9 | Configurar IA | Provedor e chave do usuário, instruções próprias, privacidade e editor do contexto enviado (cada parte pode ser desligada ou editada). |

## Escopo

### MVP

- Questionário ANBIMA, alocação-alvo editável, objetivo em texto livre, metas
- Lançamento manual com campos fechados, importação da B3 com comparação, cotações automáticas
- Painel da carteira com todos os gráficos, saudação e notícias com link
- Detalhe da classe com benchmark
- Insights no painel, próximo aporte por classe, chat por classe, assistente
- Fontes citadas com link, contexto de mercado, instruções próprias, editor de contexto
- Integração com Claude (chave do usuário) e controles de privacidade

### V2

- IA sugere metas a partir do objetivo
- Importar exportação das corretoras (depois de pesquisar os formatos)
- Ler nota de corretagem em PDF com IA
- Proventos e calendário de vencimentos detalhados
- OpenAI, Gemini e modelos locais (Ollama)
- Histórico de decisões de aporte

### Depois

- Open Finance
- Cálculo de IR e DARF
- Comparar respostas de duas IAs
- Alertas no computador ou por e-mail

## Decisões

| Tema | Decisão |
|------|---------|
| Plataforma | Só computador. Nada mobile. |
| Distribuição | Programa instalado. Carteira e chave da IA ficam no computador do usuário, sem servidor no MVP. Instaladores para Windows (`.exe`), macOS (`.dmg` para Apple Silicon e para Intel) e Linux (`.deb` e AppImage), na página de versões do GitHub, sem assinatura por enquanto. Atualização: o app procura uma versão nova ao abrir, com o ajuste para desligar em "Sobre e avisos"; nada é baixado sem clique, e a instalação por clique existe só no Windows e no AppImage. |
| Tecnologia | Electron com interface em HTML, CSS e JS puros, e núcleo em Rust. Detalhes em [`arquitetura.md`](arquitetura.md). |
| Perfil | Questionário próprio de 12 perguntas nas dimensões da ANBIMA (objetivos, situação financeira, conhecimento), sem prévia do resultado, com travas por prazo e reserva e avisos de inconsistência. Três perfis: Conservador, Moderado e Arrojado. |
| Regulatório | A IA apresenta opções, prós e contras e fontes. A decisão é sempre do usuário. Os avisos legais ficam numa fonte só, em [`avisos.md`](avisos.md) e na tela "Sobre e avisos". Revisão por advogado pendente antes da 1.0. |
| Dados de mercado | Indicadores buscados pelo próprio app nas fontes oficiais. Busca na web da IA só para notícias. |
| Custo da IA | O usuário usa e paga a própria IA. O app só integra. |
| Cotações | Arquivo diário oficial da B3 (série histórica), com o preço de fechamento do último pregão. Sem preço em tempo real no MVP. |
| Valor atual | Bruto, antes do IR, calculado pelos índices oficiais (renda fixa), pelo preço do Tesouro e pela cotação de fechamento da B3. Fundos com CNPJ usam a cota diária da CVM, que sai com alguns dias de atraso; fundos sem CNPJ, previdência e cripto ficam pelo saldo do último extrato importado ou, sem ele, pelo valor aplicado. |
| Ibovespa e IFIX | Comparação pelos ETFs BOVA11 e XFIX11, cujas cotações vêm do mesmo arquivo da B3. As cotações de referência são sempre baixadas, mesmo sem ações ou FIIs na carteira. |
| Saudação | Escrita pelo app, sorteada de uma lista fixa de frases. Não usa a IA nem gasta tokens. |
| Contexto da IA | Cada parte do contexto (instruções, perfil, carteira, mercado, metas) pode ser desligada ou editada. |
| IA | Claude pela Messages API, com a chave do usuário guardada no cofre do sistema e conferida ao salvar. Modelos Opus 5.5 (padrão), Sonnet 5.5 e Haiku 5.5, que o usuário troca em Configurar IA. Privacidade por padrão: a carteira e as metas vão só em percentuais, sem nome de instituição, e cada parte do contexto pode ser desligada. O contexto é montado quando a conversa nasce e fica congelado nela. Busca na web da própria Anthropic, com cada fonte numerada na resposta e listada com link e data no painel "O que a IA está vendo". Uso do mês em tokens e custo estimado em dólar, sem contar buscas. Insights e notícias do painel só são gerados por clique, porque cada geração gasta a chave do usuário, e ficam guardados com a data até a próxima geração. Os insights usam saída estruturada e só os dados do contexto, sem busca na web. As notícias vêm da busca na web, e cada uma só existe com o link da matéria que a resposta citou. |
| Classes | Renda fixa (inclui Tesouro), Ações (inclui ETFs), FIIs, Exterior (BDRs) e Outros (fundos, previdência e cripto). A classe Caixa do protótipo fica para quando houver um tipo para ela. |
| Rentabilidade da carteira | Retorno mensal pelo método de Dietz modificado (os aportes pesam pelos dias em que ficaram aplicados no mês), encadeado no período. Investimento sem preço conta pelo valor aplicado, e o mês em que ele ganha preço não vira rendimento. Benchmarks comparados a partir do primeiro aporte do período. Histórico de 6 ou 12 meses, pelos fechamentos de fim de mês. |
| Proventos | Fora do painel até haver fonte (importação da B3 ou V2). |
| Importação | Planilha de posição da Área do Investidor B3 (.xlsx), lida pelo nome das abas e das colunas. Casa por ticker (renda variável), código ou emissor + vencimento (renda fixa) e título + vencimento (Tesouro). Valor aplicado, taxa e preço médio não vêm na posição: o usuário completa na tela antes de gravar. Nada é apagado sem escolha explícita. |
| Próximo aporte | Divide o valor digitado entre as classes da alocação-alvo, por nivelamento: todas as classes com alvo sobem até a mesma fração do seu alvo, e a classe que já passou dessa fração recebe 0. Só aporte, nunca venda: a soma das sugestões é o valor digitado, ao centavo. O valor de cada classe é o atual da carteira (sem cotação, o aplicado). Sem histórico: nada é gravado, e o histórico de decisões de aporte é da V2. A direção que o usuário quer dar ao dinheiro (renda fixa, ações, algo novo, em texto livre também) e a meta ligada ao aporte vão só para o contexto do chat por classe e não mudam a divisão. "Perto do alvo" no Próximo aporte significa dentro de 1 p.p. (na Carteira, "no alvo" é ±0,5 p.p.). Sem alocação-alvo, a tela convida a defini-la. |

## Em aberto


## Identidade visual

- Verde principal `#04321F`, sempre com textura granulada e degradê suave, no estilo do navegador Zen: moldura da janela, menu lateral, botões, mensagens do usuário.
- Conteúdo em painéis claros flutuantes com fundo de papel quadriculado leve, cantos recortados em pixel, títulos em caixa alta e traço fino (IBM Plex Sans), rótulos em IBM Plex Mono.
- Barras de progresso inteiras com cantos retos. Grade pontilhada nos gráficos.
