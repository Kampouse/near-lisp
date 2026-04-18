;; 04-lambdas.lisp — lambda, closures, define shorthand

;; Basic lambda
(define double (lambda (x) (* x 2)))
(double 21)  ;; => 42

;; Multi-arg lambda
(define add3 (lambda (a b c) (+ a b c)))
(add3 1 2 3)  ;; => 6

;; Define shorthand (sugar for lambda)
(define (square x) (* x x))
(square 7)  ;; => 49

(define (max-of a b) (if (> a b) a b))
(max-of 10 20)  ;; => 20

;; Closures — lambda captures its environment
(define (make-adder n)
  (lambda (x) (+ n x)))

(define add5 (make-adder 5))
(define add10 (make-adder 10))
(add5 3)    ;; => 8
(add10 3)   ;; => 13

;; Closure over mutable state pattern (via redefinition)
(define (make-counter)
  (let ((count 0))
    (lambda ()
      ;; Returns count, can't mutate directly
      ;; but demonstrates closure capture
      count)))

;; Higher-order functions
(define (apply-twice f x) (f (f x)))
(apply-twice double 3)  ;; => 12 (3→6→12)

;; Function as argument
(define (compose f g) (lambda (x) (f (g x))))
(define add1 (lambda (x) (+ x 1)))
(define mul2 (lambda (x) (* x 2)))
(define add1-then-mul2 (compose mul2 add1))
(add1-then-mul2 4)  ;; => 10 ((4+1)*2)
