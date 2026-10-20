# Proof: language-switch

- feature: language-switch
- entry: GET / with Cookie lang=en|pt-br (same as data-lang buttons in the topbar)
- origin: http://127.0.0.1:18100
- result: pass
- observed: default HTML is pt-BR (`Gerar`, brand string); `lang=en` yields
  `RoboHash Avatar Generator` / `Generate` / `html lang="en"`; pt-br restores.
- side effects: none beyond the lang cookie.
