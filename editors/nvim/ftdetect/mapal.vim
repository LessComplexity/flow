" Mapal filetype detection.
" Associates *.mapal files with the `mapal` filetype so syntax/mapal.vim loads.
autocmd BufRead,BufNewFile *.mapal setfiletype mapal
