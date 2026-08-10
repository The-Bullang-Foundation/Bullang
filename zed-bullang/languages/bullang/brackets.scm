; A string is one token in this grammar — its quotes are not separate nodes —
; so there is no quote pair to match here.
("(" @open ")" @close)
("{" @open "}" @close)
("[" @open "]" @close)
