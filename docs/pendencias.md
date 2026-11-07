# Pendências

Lista do que ainda vai ser revisado ou construído. Cada item vira uma fase ou correção quando for começado.

- [ ] **Valor atual de fundos pela cota da CVM.** Fundo cadastrado à mão hoje fica "sem fonte de preço"; só o fundo importado do extrato da BTG tem valor. Usar a cota diária que a CVM publica por CNPJ (`inf_diario_fi` em dados.cvm.gov.br): valor atual = cotas × cota do dia. O cadastro do fundo passa a pedir o CNPJ; o número de cotas é informado ou calculado por valor aplicado ÷ cota na data da aplicação. A cota sai com alguns dias de atraso, o arquivo da CVM é grande e o valor é bruto (antes de IR e come-cotas). O saldo do extrato da BTG continua valendo para os fundos importados.
- [ ] **Respostas do Assistente formatadas.** As respostas chegam em texto com marcação (tabelas, listas, negrito) e a tela mostra o texto cru: uma tabela aparece toda quebrada. A tela deve exibir a resposta já formatada.
