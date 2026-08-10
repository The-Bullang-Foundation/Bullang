" Bullang filetype detection.
"
" Both extensions are declared here so a single `runtimepath` entry covers the
" family: .bu is Bullang, .busc is BullScript. They are different languages
" with different syntax files.
au BufRead,BufNewFile *.bu   set filetype=bullang
au BufRead,BufNewFile *.busc set filetype=bullscript
