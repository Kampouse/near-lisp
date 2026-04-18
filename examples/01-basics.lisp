;; 01-basics.lisp — Primitives, arithmetic, comparisons
;; Run: near-lisp eval "$(cat examples/01-basics.lisp)"

;; Numbers (integers and floats)
(+ 1 2 3)           ;; => 6
(- 10 3)             ;; => 7
(* 4 5)              ;; => 20
(/ 10 3)             ;; => 3.333... (float division)
(/ 10 2)             ;; => 5 (integer when exact)
(mod 10 3)           ;; => 1

;; Nested arithmetic
(* (+ 2 3) (- 10 4)) ;; => 30

;; Booleans
true                  ;; => true
false                 ;; => false
nil                   ;; => nil

;; Comparisons
(= 5 5)              ;; => true
(!= 3 4)             ;; => true
(< 1 2)              ;; => true
(>= 5 5)             ;; => true
(> 10 3)             ;; => true

;; Strings
"hello world"         ;; => "hello world"
(str-concat "hello" " " "world")  ;; => "hello world"
(str-length "near")   ;; => 4
(str-contains "near-lisp" "lisp") ;; => true
(str-substring "hello" 1 3)       ;; => "el"
(str-split "a,b,c" ",")           ;; => ("a" "b" "c")
