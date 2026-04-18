;; 07-pattern-matching.lisp — match, destructuring

;; Basic match on values
(define (describe x)
  (match x
    0 "zero"
    1 "one"
    "hello" "greeting"
    _ "something else"))
(describe 0)        ;; => "zero"
(describe 1)        ;; => "one"
(describe "hello")  ;; => "greeting"
(describe 42)       ;; => "something else"

;; Destructuring lists
(match (list 1 2 3)
  (a b c) (+ a b c))  ;; => 6 — bind a=1, b=2, c=3

;; Match with rest pattern
(define (head-tail lst)
  (match lst
    () "empty"
    (first . rest) first))
(head-tail (list 1 2 3))  ;; => 1
(head-tail (list))         ;; => "empty"

;; Nested destructuring
(match (list 1 (list 2 3))
  (a (b c)) (+ a b c))    ;; => 6

;; Match with guards (using if in cond)
(define (classify-list lst)
  (match lst
    () "empty"
    (x) "single"
    (x y) "pair"
    _ "many"))
(classify-list (list))           ;; => "empty"
(classify-list (list 1))          ;; => "single"
(classify-list (list 1 2))        ;; => "pair"
(classify-list (list 1 2 3))      ;; => "many"
