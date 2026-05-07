# Scooter — Vim Bindings

This document covers two topics:

1.  **[Vim-style TUI key bindings](#vim-style-tui-key-bindings)** — key bindings
    inside scooter's own TUI that follow vim conventions, configured via
    `vim-config.toml`.
2.  **[Vim/Neovim editor integration](#vimneovim-editor-integration)** — mappings
    for launching scooter from inside Vim using the
    [VimRun](https://github.com/nkh/VimRun) plugin.

---

## Vim-style TUI key bindings

The file [`vim-config.toml`](vim-config.toml) provides a complete set of
vim-inspired key bindings for scooter's TUI.  To use it:

```sh
scooter --config zcooter/vim-config.toml
```

Or copy it to `~/.config/scooter/config.toml` to make it the default.

### How scooter's "modes" work

Scooter does not have a true modal editor like Vim.  Instead it has two
**focus states** that map naturally to vim modes:

| Scooter focus         | Vim equivalent | What you do                              |
|----------------------|----------------|------------------------------------------|
| **Fields focused**   | Insert mode    | Type text into search/replace/include fields |
| **Results focused**  | Normal mode    | Navigate results, toggle selections, open files |

When scooter starts, focus is on the search field (insert mode).  Pressing
`Enter` triggers the search and focus moves to the results list (normal mode).
Press `i` or `backspace` to return to insert mode.

### Key binding syntax

Individual keys can be:

| Syntax             | Meaning                        | Examples         |
|--------------------|--------------------------------|------------------|
| `a`                | Bare letter                    | `j`, `k`, `/`    |
| `C-x`              | Control + key                  | `C-c`, `C-d`     |
| `A-x`              | Alt + key                      | `A-i`, `A-u`     |
| `S-x`              | Shift + key (uppercase letter) | `G`, `J`         |
| `enter`, `tab` ... | Named special keys             | `backspace`, `esc` |
| `:x`               | Colon-prefix two-key sequence  | `:q`, `:h`, `:e` |
| `xy`               | Leader-key two-key sequence    | `zl`, `zh`, `zf` |

Multiple keys can be bound to the same command by using a TOML array:
`quit = [":q", "C-c"]`.

### Two-key sequences

Scooter supports two styles of two-key (chord) sequences:

1.  **Colon-prefixed** (`:x`) — modelled after vim's `:command` system.
    Press `:` then a letter.  Used for top-level commands like `:q` (quit),
    `:h` (help), `:r` (reset).

2.  **Leader-key** (`xy`) — two letters typed in quick succession where the
    first letter acts as a "leader".  In the vim config, `z` is used as the
    leader for toggle commands:

    | Binding | Action                                   |
    |---------|------------------------------------------|
    | `zl`    | Toggle line wrapping in the preview pane |
    | `zh`    | Toggle hidden files (dotfiles)           |
    | `zm`    | Toggle multiline regex search            |
    | `ze`    | Toggle escape-sequence interpretation    |
    | `zf`    | Focus the fixed-strings toggle field     |

### Default vs vim key bindings

The tables below compare every command's binding in the default config
(`default-config.toml`) versus the vim-style config (`vim-config.toml`).
An entry in parentheses means the binding exists only in that config.

#### General commands (all screens)

| Command          | Default              | Vim                   |
|------------------|----------------------|-----------------------|
| quit             | `C-c`                | `:q`, `C-c`           |
| reset            | `C-r`                | `:r`                  |
| show help menu   | `C-h`                | `:h`, `?`             |

#### Search screen — common (both field and result focus)

| Command                          | Default          | Vim                  |
|----------------------------------|------------------|----------------------|
| toggle preview wrapping          | `C-l`            | `zl`                 |
| toggle hidden files              | `C-t`            | `zh`                 |
| toggle multiline                 | `A-m`            | `zm`                 |
| toggle escape sequences          | `A-e`            | `ze`                 |
| shrink column                    | `C-left`         | `C-left`, `<`        |
| grow column                      | `C-right`        | `C-right`, `>`       |

#### Search screen — fields focused (insert mode)

| Command                  | Default           | Vim                     |
|--------------------------|-------------------|-------------------------|
| trigger search           | `enter`           | `enter`                 |
| focus next field         | `tab`             | `tab`                   |
| focus previous field     | `S-tab`           | `S-tab`                 |
| unlock prepopulated      | `A-u`             | `A-u`                   |
| open file finder         | `A-f`             | `A-f`                   |
| focus search field       | `C-s`             | `/`                     |
| focus replace field      | `C-r`             | `%`                     |
| focus include field      | `C-i`             | `A-i`                   |
| focus exclude field      | `C-e`             | `A-e`                   |
| focus fixed field        | `C-t`             | `zf`                    |
| fields to results        | `down` / `up`     | `down` / `up`           |

#### Search screen — results focused (normal mode)

| Command                  | Default                      | Vim                           |
|--------------------------|------------------------------|-------------------------------|
| trigger replacement      | `enter`                      | `enter`                       |
| back to fields           | `C-o`                        | `C-o`                         |
| open in editor           | `e`                          | `:e`, `e`                     |
| move down                | `j`, `C-n`, `down`           | `j`, `C-n`, `down`            |
| move up                  | `k`, `C-p`, `up`             | `k`, `C-p`, `up`              |
| next file                | `C-down`                     | `C-down`, `J`                 |
| previous file            | `C-up`                       | `C-up`, `K`                   |
| half page down           | `C-d`                        | `C-d`                         |
| half page up             | `C-u`                        | `C-u`                         |
| full page down           | `C-f`, `pagedown`            | `C-f`, `pagedown`             |
| full page up             | `C-b`, `pageup`              | `C-b`, `pageup`               |
| top                      | `g`                          | `g`, `home`                   |
| bottom                   | `G`                          | `G`, `end`                    |
| toggle selection         | `space`                      | `space`                       |
| toggle all               | `C-w`                        | `a`                           |
| toggle current file      | `*`                          | `*`                           |
| multiselect mode         | `v`                          | `v`                           |
| flip multiselect dir     | `A-;`                        | `o`                           |
| enter insert mode        | —                            | `i`                           |
| backspace to search      | —                            | `backspace`                   |

#### Results screen (post-replacement)

| Command          | Default                    | Vim                     |
|------------------|----------------------------|-------------------------|
| scroll errors down | `j`, `down`, `C-n`      | `j`, `down`, `C-n`      |
| scroll errors up   | `k`, `up`, `C-p`        | `k`, `up`, `C-p`        |
| quit               | `enter`, `q`             | `enter`, `q`            |

### Notable differences from vim

- **No command-line mode** — scooter has no `:` command line.  The `:x`
  bindings are simple two-key sequences, not a text input prompt.
- **Arrow keys always work** — in the vim config, arrow keys are kept as
  fallback bindings alongside `h`/`j`/`k`/`l` equivalents.
- **Tab still cycles fields** — unlike vim's tab handling, `Tab` and
  `Shift-Tab` cycle between the search fields as in the default config.
- **No `w`/`b` word motion** — these are not implemented; use `C-d`/`C-u`
  for half-page scrolling instead.

---

## Vim/Neovim editor integration

This section describes how to integrate scooter with Vim/Neovim using the
[VimRun](https://github.com/nkh/VimRun) plugin, which provides the `RunSplit`
and `RunSilent` commands used by the bindings below.

### Prerequisites

Install [VimRun](https://github.com/nkh/VimRun) and source it from your `.vimrc`
or `init.vim`. The two commands it provides are:

#### `RunSplit` — Run a command in a split

Open an external command in a Vim terminal split. The split is automatically
closed on exit (code 0) unless the `-k` flag is used.

```
:RunSplit [flags] <command>
```

| Flag | Description |
|------|-------------|
| `-v` | Vertical split (default) |
| `-h` | Horizontal split |
| `-k` | Keep split open after the command exits |
| `N`  | Split size as a percentage (0–100, default: 50) |

Flags and the percentage can appear in any order. The first non-flag argument
starts the command.

Example — open scooter in a 30% vertical split:

```
:RunSplit 30 scooter
```

#### `RunSilent` — Run a command silently

Run an external command with `:!` silently (no "Press ENTER" prompt), then
redraw the screen.

```
:RunSilent <command>
```

### Auto-reload files

Add the following to your Vim config to auto-reload files changed on disk (e.g. after running scooter):

```vim
set autoread
autocmd FocusGained,BufEnter * checktime
```

### Bindings

Add the following to your `.vimrc` (or `init.vim` for Neovim):

```vim
" scooter --------------------------------------------------------------------

" Run scooter fullscreen (no split)
function! Scooter(...) abort
  if a:0 == 0
    call RunSilent('scooter')
  else
    let l:cmd = 'scooter ' . join(a:000, ' ')
    call RunSilent(l:cmd)
  endif
endfunction
command! -nargs=* Scooter call Scooter(<f-args>)

" Run scooter in a vertical split
function! VScooter(...) abort
  if a:0 == 0
    call RunSplit('scooter')
  else
    let l:cmd = 'scooter ' . join(a:000, ' ')
    call RunSplit(l:cmd)
  endif
endfunction
command! -nargs=* -bar VScooter call VScooter(<f-args>)

nnoremap <leader>SS :Scooter<CR>
nnoremap <leader>SV :VScooter<CR>
nnoremap <leader>SA :VScooter
```

#### Choosing the default mode

The `g:scooter_cmd` variable controls whether the file/repo/selection bindings
below open scooter fullscreen or in a split. Set it to either `Scooter` or
`VScooter`:

```vim
" Choose 'Scooter' (fullscreen) or 'VScooter' (vertical split)
let g:scooter_cmd = 'VScooter'
```

#### File, repo, and selection bindings

```vim
" Open scooter for the current file
nnoremap <leader>SF :execute g:scooter_cmd . ' ' . fnameescape(expand('%'))<CR>

" Open scooter for all files in the current git repo
nnoremap <leader>SG :execute g:scooter_cmd trim(system('git rev-parse --show-toplevel'))<CR>

" Open scooter with visual selection as the search text
vnoremap <leader>SC :<C-U>execute g:scooter_cmd . ' scooter --file-list-height 10 --fixed-strings --no-file-filters --editable --search-text ' . fnameescape(@")<CR>

" Open scooter with word under cursor as the search text
nnoremap <leader>SC :execute g:scooter_cmd . ' --file-list-height 10 --editable --no-file-filters --fixed-strings --search-text ' . fnameescape(expand('<cword>'))<CR>
```

#### Key reference

| Mapping | Mode | Description |
|---------|------|-------------|
| `<leader>SS` | Normal | Open scooter fullscreen |
| `<leader>SV` | Normal | Open scooter in a vertical split |
| `<leader>SA` | Normal | Open scooter in a vertical split with trailing space (for typing args) |
| `<leader>SF` | Normal | Open scooter for the current file |
| `<leader>SG` | Normal | Open scooter for the current git repo |
| `<leader>SC` | Normal | Open scooter with the word under cursor as search text |
| `<leader>SC` | Visual | Open scooter with the visual selection as search text |

### Tips

- Use `g:scooter_cmd = 'Scooter'` if you prefer scooter to always open
  fullscreen.
- The `--no-file-filters` flag in the selection bindings hides the
  include/exclude fields, saving vertical space.
- `--fixed-strings` treats the search text as a literal string instead of a
  regex, which is more intuitive for selected text.
- `--editable` allows modifying the pre-populated search text inside the TUI.
- `--file-list-height 10` increases the file list height in narrow terminal
  layouts.
