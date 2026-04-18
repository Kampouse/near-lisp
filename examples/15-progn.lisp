;; 15-progn.lisp — sequencing, begin, progn

;; progn / begin — evaluate multiple expressions, return last
(progn
  (define a 1)
  (define b 2)
  (+ a b))  ;; => 3

;; Useful in branches
(define result
  (if true
    (progn
      (define x 10)
      (define y 20)
      (+ x y))
    0))
result  ;; => 30

;; Nested progn
(progn
  (define step1 (+ 1 2))
  (progn
    (define step2 (* step1 10))
    step2))  ;; => 30
