# Scooter — Vim Bindings

This document describes how to integrate scooter with Vim/Neovim using the
[VimRun](https://github.com/nkh/VimRun) plugin, which provides the `RunSplit`
and `RunSilent` commands used by the bindings below.

## Prerequisites

Install [VimRun](https://github.com/nkh/VimRun) and source it from your `.vimrc`
or `init.vim`. The two commands it provides are:

### `RunSplit` — Run a command in a split

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

### `RunSilent` — Run a command silently

Run an external command with `:!` silently (no "Press ENTER" prompt), then
redraw the screen.

```
:RunSilent <command>
```

## Auto-reload files

Add the following to your Vim config to auto-reload files changed on disk (e.g. after running scooter):

set autoread
autocmd FocusGained,BufEnter * checktime

## Bindings

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

### Choosing the default mode

The `g:scooter_cmd` variable controls whether the file/repo/selection bindings
below open scooter fullscreen or in a split. Set it to either `Scooter` or
`VScooter`:

```vim
" Choose 'Scooter' (fullscreen) or 'VScooter' (vertical split)
let g:scooter_cmd = 'VScooter'
```

### File, repo, and selection bindings

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

### Key reference

| Mapping | Mode | Description |
|---------|------|-------------|
| `<leader>SS` | Normal | Open scooter fullscreen |
| `<leader>SV` | Normal | Open scooter in a vertical split |
| `<leader>SA` | Normal | Open scooter in a vertical split with trailing space (for typing args) |
| `<leader>SF` | Normal | Open scooter for the current file |
| `<leader>SG` | Normal | Open scooter for the current git repo |
| `<leader>SC` | Normal | Open scooter with the word under cursor as search text |
| `<leader>SC` | Visual | Open scooter with the visual selection as search text |

## Tips

- Use `g:scooter_cmd = 'Scooter'` if you prefer scooter to always open
  fullscreen.
- The `--no-file-filters` flag in the selection bindings hides the
  include/exclude fields, saving vertical space.
- `--fixed-strings` treats the search text as a literal string instead of a
  regex, which is more intuitive for selected text.
- `--editable` allows modifying the pre-populated search text inside the TUI.
- `--file-list-height 10` increases the file list height in narrow terminal
  layouts.
