" Flow filetype detection.
" Associates *.flow files with the `flow` filetype so syntax/flow.vim loads.
autocmd BufRead,BufNewFile *.flow setfiletype flow
