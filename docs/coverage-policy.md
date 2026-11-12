# Cobertura — política e exclusões declaradas

O gate de cobertura exige **100% de linhas e 100% de regiões** no código próprio.

## Exclusões declaradas

| Arquivo | Motivo |
|---|---|
| `src/main.rs` | Glue de bootstrap: monta o subscriber de log, cria o store, dispara o loop de limpeza, o aquecimento de previews e chama `axum::serve`, que só retorna quando o processo termina. Nenhuma decisão de produto mora aqui; a lógica testável está em `lib.rs` e nos módulos. |

Qualquer outra exclusão precisa entrar nesta tabela com o motivo, e não pode
esconder regra de negócio. Ramo novo que exista porque o código pode falhar
precisa de teste que provoque a falha real.

## Como o gate mede

`.github/workflows/ci.yml` roda `cargo llvm-cov --all-targets` e depois
`cargo llvm-cov report`, ignorando apenas `src/main.rs` pelo
`--ignore-filename-regex`. O restante — `src/*.rs` e `tests/integration.rs` —
entra na conta, inclusive o código de teste dos módulos.

A régua subiu de 90% para 100% quando o código passou a ter 100%.
