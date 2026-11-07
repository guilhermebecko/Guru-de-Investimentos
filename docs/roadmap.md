# Roteiro

Ordem de construção do MVP descrito em [`mvp.md`](mvp.md). Cada fase termina com o app abrindo e fazendo algo novo de ponta a ponta. Cada uma ganha o próprio plano em `docs/superpowers/plans/` quando for começar.

| Fase | Entrega | Telas do protótipo |
|------|---------|--------------------|
| 0. Fundação | Janela do Electron com a moldura verde e o menu lateral, núcleo Rust carregado, banco SQLite criado e CI nas três plataformas. | moldura |
| 1. Carteira manual | Lançar, editar e apagar investimentos com campos fechados, IR e garantia preenchidos pelo tipo. Lista por classe e totais. | 4, 6 (sem gráfico) |
| 2. Perfil e objetivo | Questionário ANBIMA sem prévia, perguntas de consistência, alocação-alvo editável, objetivo em texto livre e metas. | 1, 2 |
| 3a. Dados de mercado | Selic, CDI, IPCA e Tesouro das fontes oficiais, cotações da B3, valor atual, rentabilidade e atualização de mercado. | 6 (sem gráfico) |
| 3b. Painel | Gráficos do painel: patrimônio × aportado ao longo do tempo, comparação com CDI, IPCA, BOVA11 e XFIX11, mês a mês, instituições, vencimentos, altas e quedas, e a saudação. | 5, 6 (gráficos) |
| 4. Importação da B3 | Ler a planilha da Área do Investidor e comparar com o cadastrado, linha a linha. | 3 |
| 5. Próximo aporte | Divisão do valor por classe por rebalanceamento, sem IA. | 7 (sem chat) |
| 6a. IA e Assistente | Chave no cofre do sistema, modelo, privacidade, instruções próprias, editor de contexto e o Assistente com fontes numeradas. | 8, 9 |
| 6b. IA no resto do app | Chat por classe no Próximo aporte, insights e notícias no painel. | 5, 7 |
| 7. Distribuição | Instaladores para Windows, macOS e Linux, atualização automática e revisão jurídica dos textos. | — |
| 8. Importação da BTG | Ler o extrato em PDF da BTG e comparar com o cadastrado; um investimento por corretora nas duas importações; valor atual de fundos e cripto pelo extrato. | 3 |
| 9. Fundos pela CVM e Assistente formatado | Valor atual de fundos com CNPJ pela cota diária da CVM e respostas do Assistente formatadas (tabelas, listas, código, citação). | 3, 6, 8 |

A revisão jurídica dos textos por um advogado é pendente e condição para a versão 1.0. A fase 7 consolidou os avisos em [`avisos.md`](avisos.md).

Os planos detalhados:

- Fase 0: [`superpowers/plans/2026-10-28-fase-0-fundacao.md`](superpowers/plans/2026-10-28-fase-0-fundacao.md)
- Fase 1: [`superpowers/plans/2026-10-28-fase-1-carteira-manual.md`](superpowers/plans/2026-10-28-fase-1-carteira-manual.md)
- Fase 2: [`superpowers/plans/2026-10-29-fase-2-perfil-objetivo.md`](superpowers/plans/2026-10-29-fase-2-perfil-objetivo.md)
- Fase 3a: [`superpowers/plans/2026-10-29-fase-3a-dados-de-mercado.md`](superpowers/plans/2026-10-29-fase-3a-dados-de-mercado.md)
- Fase 3b: [`superpowers/plans/2026-10-30-fase-3b-painel.md`](superpowers/plans/2026-10-30-fase-3b-painel.md)
- Fase 4: [`superpowers/plans/2026-10-30-fase-4-importacao-b3.md`](superpowers/plans/2026-10-30-fase-4-importacao-b3.md)
- Fase 5: [`superpowers/plans/2026-10-30-fase-5-proximo-aporte.md`](superpowers/plans/2026-10-30-fase-5-proximo-aporte.md)
- Fase 6a: [`superpowers/plans/2026-10-30-fase-6a-ia-assistente.md`](superpowers/plans/2026-10-30-fase-6a-ia-assistente.md)
- Fase 6b: [`superpowers/plans/2026-11-03-fase-6b-ia-no-app.md`](superpowers/plans/2026-11-03-fase-6b-ia-no-app.md)
- Fase 7: [`superpowers/plans/2026-11-03-fase-7-distribuicao.md`](superpowers/plans/2026-11-03-fase-7-distribuicao.md)
- Fase 8: [`superpowers/plans/2026-11-06-fase-8-importacao-btg.md`](superpowers/plans/2026-11-06-fase-8-importacao-btg.md)
- Fase 9: [`superpowers/plans/2026-11-07-fase-9-fundos-cvm-e-markdown.md`](superpowers/plans/2026-11-07-fase-9-fundos-cvm-e-markdown.md)
