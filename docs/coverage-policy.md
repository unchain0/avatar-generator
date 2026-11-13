# Cobertura — política e exclusões declaradas

## Regra

1. **Código próprio: 100% de linhas e regiões por arquivo**, com as exclusões de
   arquivo declaradas abaixo.
2. Caminho de pânico de `expect` de invariante e interior de macro do `tracing`
   **não contam** para o piso: em stable o `llvm-cov` não permite excluir linha.
   Por isso o gate tem dois números, e nenhum deles é 100%:
   - piso total **≥ 99,85%** de linhas e de regiões;
   - **nenhum arquivo abaixo de 99,6%** em linhas ou regiões sem justificativa
     escrita nesta política.
3. **É proibido trocar tratamento de erro real por descarte silencioso para
   subir número.** Se aparecer a tentação, o caminho é declarar a exclusão com
   motivo aqui.

## Exclusões declaradas

| Arquivo | Motivo |
|---|---|
| `src/main.rs` | Glue de bootstrap: monta o subscriber de log, cria o store, dispara o loop de limpeza, o aquecimento de previews e chama `axum::serve`, que só retorna quando o processo termina. Nenhuma decisão de produto mora aqui; a lógica testável está em `lib.rs` e nos módulos. |
| `src/security.rs` (só regiões: 99,42%) | Um único registro de região vem do `poll_ready` delegado ao serviço interno, que neste app nunca fica pendente; linhas seguem 100% e o caminho de erro do middleware é coberto por `inner_errors_are_propagated_unchanged`. Justificado para ficar abaixo do piso por arquivo (99,6%), acima do piso total. |
| `/registry/` (dependências) | Código de terceiros: as linhas do `tracing` que o `llvm-cov` atribui à expansão dos macros não são código deste repositório e não entram no piso. |

Qualquer outra exclusão precisa entrar nesta tabela com o motivo, e não pode
esconder regra de negócio. Ramo novo que exista porque o código pode falhar
precisa de teste que provoque a falha real.

## Como o gate mede

`.github/workflows/ci.yml` roda `cargo llvm-cov --all-targets` e depois
`cargo llvm-cov report --json`, ignorando `src/main.rs` e as dependências pelo
`--ignore-filename-regex`. O restante — `src/*.rs` e `tests/integration.rs` —
entra na conta, inclusive o código de teste dos módulos.

O script confere os dois números da regra: total ≥ 99,85% e cada arquivo
≥ 99,6%. A régua subiu de 90% para 99,85% quando o código chegou lá; a
diferença até 100% é exatamente o que o item 2 descreve.

Medição de referência (2026-11-12, com as exclusões acima): total **99,88%** de
linhas e **99,89%** de regiões.

## Resíduo de instrumentação declarado (2026-11-13)

`llvm-cov` conta, em `src/handlers.rs` e `src/store.rs`, **2 linhas e 4/2
regiões em cada** que não existem no arquivo: são trechos de expansão de macro
e caminhos de pânico de `expect` de invariante. Evidência:

- `cargo llvm-cov --html`: **0** linhas `uncovered` em ambos os arquivos;
- `cargo llvm-cov --lcov`: nenhuma linha `DA:...,0` em ambos;
- `cargo llvm-cov report --json`: nenhum segmento com `count == 0`.

Mesmo assim o resumo do `llvm-cov` desconta essas linhas do total. Elas ficam
declaradas aqui, com este motivo, em vez de virarem teste artificial. As linhas
reais desses arquivos seguem cobertas; o piso total e o piso por arquivo
continuam valendo.
