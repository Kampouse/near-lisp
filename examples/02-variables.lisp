;; 02-variables.lisp — define, let, set!
;; Variables and scoping

;; Global define
(define x 42)
(define greeting "hello")
x         ;; => 42
greeting  ;; => "hello"

;; Define with expression
(define y (* 6 7))
y  ;; => 42

;; Local bindings with let
(let ((a 10) (b 20))
  (+ a b))  ;; => 30

;; let doesn't leak scope
;; a  ;; => ERROR: undefined variable

;; Nested let
(let ((x 1))
  (let ((x 2) (y x))   ;; y binds to outer x = 1
    (+ x y)))            ;; => 3

;; Redefine a variable
(define counter 0)
(define counter (+ counter 1))
counter  ;; => 1
