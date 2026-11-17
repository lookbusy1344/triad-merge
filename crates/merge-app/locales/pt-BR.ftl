# The windows' text in Brazilian Portuguese. `en.ftl` is the reference: it says how counts and
# names work. Portuguese counts 0 and 1 as singular (`[one]`).

## About window

about-title = Sobre o triad
about-description = Mesclagem de três vias e comparação de duas vias
about-vcs-support = Feito para Jujutsu, com suporte completo a Git
about-version = Versão { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, alteração { $change }

## Window parts

result-title = Resultado
merge-toolbar = Ferramentas de mesclagem
diff-toolbar = Ferramentas de comparação
input-lines = Linhas de entrada
go-to-line-field = Número da linha
no-matches = Nenhuma ocorrência
close = Fechar
settings-errors = O arquivo de configuração tem erros: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Salvar com conflitos não resolvidos?
prompt-save-with-conflicts = Salvar com conflitos
prompt-keep-editing = Continuar editando
prompt-discard-detail = Descartar não grava nada, e a mesclagem continua não resolvida no git ou no jj.
prompt-discard = Descartar
prompt-cannot-write = Não é possível gravar a saída
prompt-cannot-write-copy = Não é possível gravar a cópia
prompt-cannot-record = O repositório não registrou o salvamento
prompt-no-output = Esta janela não tem arquivo de saída.
prompt-quit = Sair
prompt-try-again = Tentar novamente
prompt-restore-session = Restaurar a sessão anterior?
prompt-restore-detail = Uma sessão nestes arquivos terminou antes de a mesclagem ser resolvida. Suas escolhas e edições podem ser recuperadas.
prompt-restore = Restaurar
prompt-start-over = Recomeçar
prompt-cannot-restore = Não é possível restaurar a sessão
prompt-save-changes = Salvar as alterações?
prompt-cannot-save = Não é possível salvar o arquivo
prompt-ok = OK
prompt-changed-detail = Recarregue e perca suas edições, ou mantenha sua versão e salve por cima depois.
prompt-reload = Recarregar
prompt-keep-mine = Manter a minha
prompt-changed-on-disk = { $name } mudou no disco.
prompt-descendants-changed =
    { $count ->
        [one] Salvar altera os conflitos de { $count } descendente.
       *[other] Salvar altera os conflitos de { $count } descendentes.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } conflito mantém seus marcadores, e o git ou o jj veem a mesclagem como não resolvida.
       *[other] { $count } conflitos mantêm seus marcadores, e o git ou o jj veem a mesclagem como não resolvida.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } conflito mantém seus marcadores, e o git ou o jj veem a mesclagem como não resolvida. Depois, o git mergetool devolve sua própria cópia do arquivo; execute-o de novo para restaurar esta sessão.
       *[other] { $count } conflitos mantêm seus marcadores, e o git ou o jj veem a mesclagem como não resolvida. Depois, o git mergetool devolve sua própria cópia do arquivo; execute-o de novo para restaurar esta sessão.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
count-files =
    { $count ->
        [one] { $count } arquivo
       *[other] { $count } arquivos
    }
count-versions =
    { $count ->
        [one] { $count } versão
       *[other] { $count } versões
    }
count-hunks =
    { $count ->
        [one] { $count } trecho
       *[other] { $count } trechos
    }
count-differences =
    { $count ->
        [one] { $count } diferença
       *[other] { $count } diferenças
    }
count-conflicts =
    { $count ->
        [one] { $count } conflito
       *[other] { $count } conflitos
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } linha inalterada
       *[other] { $count } linhas inalteradas
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } bloco duplicado
       *[other] { $count } blocos duplicados
    }

## Conflicted commits (stack window)

no-paths = nenhum
stack-title = Commits com conflitos
stack-empty = Nenhum commit com conflitos.
refresh = Atualizar
stack-preview = Prévia do salvamento
preview-running = Calculando o que o salvamento faria…
working-copy = cópia de trabalho
no-description = (sem descrição)
commit-tooltip =
    ID da alteração: { $change }
    ID do commit: { $commit }
    Data: { $date }
inherited = (herdado)
stack-resolved = { $path } resolvido em { $change }.
stack-saved-conflicts = { $path } salvo em { $change } com conflitos.
stack-error = A mesclagem de { $path } em { $change } terminou com um erro.
preview-resolves = Salvar resolve o caminho em { $change }.
preview-keeps = Salvar mantém conflitos em { $change }.
preview-no-rebase = Nenhum descendente é rebaseado.

## Status icons

status-added = Adicionado
status-modified = Modificado
status-deleted = Excluído
status-moved = Movido
status-copied = Copiado
status-conflicted = Em conflito

## Changed files (directory window)

dirs-title = Arquivos alterados
dirs-empty = Nenhum arquivo difere.
dirs-against = { $files } em relação a { $rev }
file-added = adicionado
file-deleted = excluído
file-modified = modificado
file-renamed = renomeado de { $from }
file-copied = copiado de { $from }
file-from = de { $from }
file-undecodable = não decodificável
file-binary = binário
file-special = especial
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ na operação { $operation }
no-file = { $name } (sem arquivo)

## History window

history-title = Histórico de { $path }
history-empty = Nenhuma versão.
version-conflict = (conflito)
version-absent = (ausente)
version-other = (outro)

## Absorb window

absorb-title = Absorver
absorb-empty = Nada para absorver.
absorb-apply = Aplicar
absorb-keep = Cópia de trabalho
absorb-note = Absorção de { $hunks } em { $commits }.
absorb-stale = A cópia de trabalho está desatualizada: execute jj workspace update-stale.
absorb-skipped = { $path } ignorado: { $reason }

## Open dialog

open-title-merge = Abrir uma mesclagem
open-title-diff = Abrir uma comparação
mode-merge = Mesclagem
mode-diff = Comparação
start-merge = Mesclar
start-diff = Comparar
slot-base = Base
slot-left = Esquerda
slot-right = Direita
slot-output = Saída
choose-base = Escolher o arquivo base
choose-left = Escolher o arquivo da esquerda
choose-right = Escolher o arquivo da direita
choose-output = Escolher o arquivo de saída
missing-slot = Escolha um arquivo para “{ $slot }”.

## Menus

menu-settings = Configurações…
menu-save = Salvar
menu-save-and-exit = Salvar e sair
menu-save-copy-as = Salvar cópia como…
menu-cancel = Cancelar
menu-edit = Editar
menu-merge = Mesclagem
menu-go = Ir
menu-view = Exibir
menu-undo = Desfazer
menu-redo = Refazer
menu-find = Localizar…
menu-go-to-line = Ir para a linha…
menu-toggle-left = Alternar esquerda
menu-toggle-base = Alternar base
menu-toggle-right = Alternar direita
menu-clear-choice = Limpar escolha
menu-take-base = Usar base
menu-take-left = Usar esquerda
menu-take-right = Usar direita
menu-take-left-then-right = Usar esquerda e depois direita
menu-take-right-then-left = Usar direita e depois esquerda
menu-take-sorted-union = Usar união ordenada
menu-take-left-all-unresolved = Usar esquerda em todos os não resolvidos
menu-take-right-all-unresolved = Usar direita em todos os não resolvidos
menu-take-whole-left = Usar o arquivo da esquerda inteiro
menu-take-whole-right = Usar o arquivo da direita inteiro
menu-carry-moved-edit = Levar a edição ao bloco movido
menu-drop-duplicate = Remover duplicata
menu-next-difference = Próxima diferença
menu-previous-difference = Diferença anterior
menu-next-conflict = Próximo conflito
menu-previous-conflict = Conflito anterior
menu-go-to-move-end = Ir à outra ponta da movimentação
menu-base-pane = Painel base
menu-blame = Autoria
menu-syntax = Realce de sintaxe
menu-soft-wrap = Quebra de linha
menu-whitespace = Mostrar espaços em branco
menu-collapse-unchanged = Recolher inalterado
menu-scroll-sync = Sincronizar rolagem do resultado

## Toolbar

tool-label-difference = Diferença
tool-label-conflict = Conflito
tool-label-left = Esquerda
tool-label-right = Direita
more = Mais
more-menu = Mais escolhas
more-base = Base
more-left-then-right = Esquerda e depois direita
more-right-then-left = Direita e depois esquerda
more-none = Nenhuma
tool-previous-difference = Diferença anterior
tool-next-difference = Próxima diferença
tool-previous-conflict = Conflito anterior
tool-next-conflict = Próximo conflito
tool-left = Alternar esquerda na região atual
tool-base = Alternar base na região atual
tool-right = Alternar direita na região atual
tool-left-then-right = Usar esquerda e depois direita na região atual
tool-right-then-left = Usar direita e depois esquerda na região atual
tool-clear = Limpar a escolha da região atual
tool-undo = Desfazer
tool-redo = Refazer
tool-base-pane = Mostrar ou ocultar o painel base
tool-find = Localizar
tool-go-to-line = Ir a uma linha do resultado
tool-syntax = Ligar ou desligar as cores de sintaxe
tool-soft-wrap = Ligar ou desligar a quebra de linha
tool-whitespace = Mostrar ou ocultar espaços em branco

## Conflict badge and pick targets

pick-label = { $title } na região { $region } de { $regions }
pick-label-unresolved = { $title } na região { $region } de { $regions }, não resolvido
badge-conflict = Conflito { $number } de { $count }
badge-sides = { $kept } de { $sides } lados
badge-all-resolved = todos resolvidos
badge-resolved-one = Conflito resolvido
badge-resolved-all = Todos os { $count } resolvidos
badge-unresolved-of = { $unresolved } de { $count } não resolvidos

## Panes

role-left = Esquerda
role-right = Direita
role-base = Base
role-side = Lado { $number }
role-base-of = Base { $number }
role-result = Resultado
cell-line = { $title }, linha { $line }: { $text }
cell-no-line = { $title }, sem linha
cell-no-line-deleted = { $title }, sem linha: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Nada escolhido ({ $count } linha descartada)
       *[other] Nada escolhido ({ $count } linhas descartadas)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } linha excluída ({ $roles })
       *[other] { $count } linhas excluídas ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } linha excluída
       *[other] { $count } linhas excluídas
    }
previous-match = Ocorrência anterior em { $title }
next-match = Próxima ocorrência em { $title }
match-count = { $number } de { $count }
go-to-line-prompt = Ir para a linha (1–{ $last }):
go-to-line-current = Linha atual { $line } de { $last }
go-to-line-target = Ir para a linha { $line }
jj-diff-banner = Estas são cópias temporárias do jj diff --tool. As edições feitas aqui se perdem quando a janela fecha.
copy-to-right = Copiar esta diferença para a direita
copy-to-left = Copiar esta diferença para a esquerda
initial-base = B
initial-left = E
initial-right = D
moved-to-base = Bloco movido: ir à outra ponta, linha { $line } da base
moved-to-left = Bloco movido: ir à outra ponta, linha { $line } da esquerda
moved-to-right = Bloco movido: ir à outra ponta, linha { $line } da direita
diff-pane-left = Esquerda: { $name }
diff-pane-right = Direita: { $name }

## Status bar

repo-git = repo git
repo-jj = repo jj
repo-none = sem repo
mode-git = mesclagem git
mode-jj = mesclagem jj
mode-git-word = mesclagem git por palavra
mode-jj-word = mesclagem jj por palavra
encoding-utf8-bom = UTF-8 com BOM
inputs-differ = { $encoding } (as entradas diferem)
word-merged =
    { $count ->
        [one] { $count } mesclado por palavra
       *[other] { $count } mesclados por palavra
    }
unresolved =
    { $count ->
        [one] { $count } não resolvido
       *[other] { $count } não resolvidos
    }
mixed-endings = finais de linha mistos
compare-whitespace = espaços significativos
compare-line-endings = finais de linha ignorados
compare-trailing = espaços finais ignorados
compare-amount = quantidade de espaços ignorada
compare-all = todos os espaços ignorados
case-ignored = maiúsculas ignoradas
patterns-ignored = padrões ignorados
blank-lines-ignored = linhas em branco ignoradas
diverge-whitespace-jj = espaços comparados de forma diferente do jj
diverge-case-git = maiúsculas ignoradas, ao contrário do git
diverge-case-jj = maiúsculas ignoradas, ao contrário do jj
diverge-patterns-git = padrões ignorados, ao contrário do git
diverge-patterns-jj = padrões ignorados, ao contrário do jj
diverge-same-change-git = alterações iguais mantidas, ao contrário do git
diverge-word-git = palavras mescladas, ao contrário do git
diverge-deep-git = conflitos divididos por dentro, ao contrário do git
diverge-deep-jj = conflitos divididos por dentro, ao contrário do jj
diverge-syntax-git = limites de sintaxe mantidos, ao contrário do git
diverge-syntax-jj = limites de sintaxe mantidos, ao contrário do jj
copy-saved =
    { $count ->
        [0] Cópia salva, resolvida
        [one] Cópia salva com { $count } conflito
       *[other] Cópia salva com { $count } conflitos
    }

## Result pane's right-click menu

choice-none = Nenhuma
choice-base = Base
choice-left = Esquerda
choice-right = Direita
choice-left-then-right = Esquerda + direita
choice-right-then-left = Direita + esquerda
action-sorted-union = União ordenada
suggested = Sugestão: { $action }
suggested-carry = Sugestão: levar a edição de { $from } à linha { $line }
suggested-drop = Sugestão: remover a duplicata da linha { $line }
edit-cut = Recortar
edit-copy = Copiar
edit-paste = Colar
edit-select-all = Selecionar tudo
