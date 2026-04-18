;; 05-lists.lisp — list, car, cdr, cons, list operations

;; Creating lists
(list 1 2 3)              ;; => (1 2 3)
(list)                     ;; => ()
(cons 0 (list 1 2 3))     ;; => (0 1 2 3) — prepend

;; Accessing elements
(car (list 1 2 3))        ;; => 1 — first element
(cdr (list 1 2 3))        ;; => (2 3) — rest
(nth 1 (list 10 20 30))   ;; => 20 — zero-indexed
(len (list 1 2 3))        ;; => 3

;; Appending lists
(append (list 1 2) (list 3 4))  ;; => (1 2 3 4)

;; Nested lists
(list (list 1 2) (list 3 4))    ;; => ((1 2) (3 4))

;; quote — literal list (not evaluated)
(quote (1 2 3))                  ;; => (1 2 3)
'(1 + 2)                         ;; => (1 + 2) — symbols preserved

;; type checking
(nil? nil)               ;; => true
(type? 42)               ;; => "number"
(type? "hi")             ;; => "string"
(type? (list 1 2))       ;; => "list"

;; Requires stdlib "list" for higher-order ops:
(require "list")

(map (lambda (x) (* x x)) (list 1 2 3 4))     ;; => (1 4 9 16)
(filter (lambda (x) (> x 2)) (list 1 2 3 4))   ;; => (3 4)
(reduce (lambda (a b) (+ a b)) 0 (list 1 2 3)) ;; => 6
(reverse (list 1 2 3))                           ;; => (3 2 1)
(sort (list 3 1 4 1 5))                          ;; => (1 1 3 4 5)
(range 0 5)                                       ;; => (0 1 2 3 4)
(zip (list 1 2 3) (list "a" "b" "c"))            ;; => ((1 "a") (2 "b") (3 "c"))

(find (lambda (x) (> x 3)) (list 1 2 4 3))      ;; => 4
(some (lambda (x) (= x 3)) (list 1 2 3))        ;; => true
(every (lambda (x) (> x 0)) (list 1 2 3))       ;; => true
