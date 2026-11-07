# Avisos

Textos legais do app. A fonte no código é `app/renderer/js/avisos.js`, e um teste confere os dois. Revisão jurídica: pendente.

Para mudar um texto, mude os dois lugares juntos: o teste `app/test/unit/avisos.test.mjs` falha se algum valor de `AVISOS` não aparecer igual numa linha de citação deste arquivo.

## Não é recomendação (`RECOMENDACAO`)

> O Guru de Investimentos é um programa de código aberto para organizar a sua carteira e conversar sobre ela com uma IA. Ele não faz recomendação individual de investimento nem presta consultoria, análise ou gestão de carteira. O app e os autores não são registrados na CVM como analistas de valores mobiliários, consultores ou administradores de carteira. As decisões de investimento são suas, e a responsabilidade por elas também.

## Inteligência artificial (`IA`)

> As respostas, os insights e as notícias são gerados por IA (Claude, da Anthropic) e podem conter erros, omissões ou informações desatualizadas. A IA apresenta opções, prós e contras e fontes; confira as fontes antes de decidir. O perfil de investidor é um questionário próprio baseado nas dimensões da ANBIMA e não substitui a análise de perfil (suitability) feita pela sua instituição.

## Dados de mercado (`MERCADO`)

> Selic, CDI e IPCA vêm do Banco Central (SGS), as taxas e os preços do Tesouro Direto vêm do Tesouro Transparente e as cotações vêm do arquivo diário da B3, com o preço de fechamento do último pregão. Não há preço em tempo real. Fundos com CNPJ usam a cota diária publicada pela CVM, que sai com alguns dias de atraso. Fundos sem CNPJ, previdência e cripto contam pelo saldo do último extrato importado ou, sem ele, pelo valor aplicado. Os valores são brutos, antes do IR, e podem diferir do extrato da sua instituição. Rentabilidade passada não garante rentabilidade futura.

## Privacidade (`PRIVACIDADE`)

> A carteira, o perfil, as metas e as conversas ficam só neste computador, e a chave da IA fica no cofre do sistema. Não há servidor do Guru. Quando você usa a IA, o app envia à Anthropic, com a sua chave, a sua pergunta e só as partes do contexto que você deixou ligadas em Configurar IA; a Anthropic trata esses dados pelos termos dela. As fontes de mercado recebem só datas e códigos. Com “Procurar atualizações ao abrir o app” ligada, o app consulta o GitHub ao abrir.

## Licença e garantia (`LICENCA`)

> Software livre sob a licença MIT, fornecido “no estado em que se encontra”, sem garantia de qualquer tipo. Os componentes de terceiros e as licenças deles estão em “Avisos de terceiros”.

## Revisão jurídica (`REVISAO_JURIDICA`)

> Versão de pré-lançamento: estes avisos ainda vão passar por revisão jurídica antes da versão 1.0.

## Rodapé do chat (`RODAPE_CHAT`)

> Conteúdo educativo gerado por IA com os seus dados e fontes da web. Pode conter erros. Não é recomendação individual de investimento. Confira as fontes.

## Rodapé dos insights (`RODAPE_INSIGHTS`)

> Gerado por IA com os seus dados. Pode conter erros. Não é recomendação individual de investimento.

## Rodapé do Próximo aporte (`RODAPE_APORTE`)

> Cálculo de rebalanceamento pela sua alocação-alvo, sem vender nada. Investimentos sem cotação contam pelo valor aplicado. Não é recomendação individual de investimento.

## Rodapé do perfil (`RODAPE_PERFIL`)

> Questionário baseado nas dimensões das regras de suitability da ANBIMA: objetivos, situação financeira e conhecimento. Não é recomendação de investimento.

## Onde aparece

| Texto | Onde |
|---|---|
| `RECOMENDACAO`, `IA`, `MERCADO`, `PRIVACIDADE`, `LICENCA` | Tela "Sobre e avisos" (`#/sobre`), um cartão para cada (`telas/sobre.js`). |
| `REVISAO_JURIDICA` | Tela "Sobre e avisos", em destaque. |
| `RODAPE_CHAT` | Rodapé do chat, na tela Assistente e na gaveta do Próximo aporte (`telas/assistente/chat.js`). |
| `RODAPE_INSIGHTS` | Rodapé da coluna de insights e notícias da Carteira (`telas/carteira/insights.js`). |
| `RODAPE_APORTE` | Rodapé do Próximo aporte (`telas/aporte.js`). |
| `RODAPE_PERFIL` | Rodapé do painel do perfil e do resultado do questionário (`telas/perfil/_comum.js`, usado por `painel.js` e `resultado.js`). |

Os avisos de terceiros (`app/avisos-terceiros.txt`) são gerados por `npm run avisos` a partir das dependências Rust e npm que vão no pacote, e aparecem na tela "Sobre e avisos" no botão "Ver avisos de terceiros". `npm run avisos:conferir` falha quando o arquivo gravado está desatualizado.
