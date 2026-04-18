;; 06-recursion.lisp — recursive functions, loop/recur for TCO

;; Classic recursion
(define (factorial n)
  (if (<= n 1) 1 (* n (factorial (- n 1)))))
(factorial 10)  ;; => 3628800

;; Fibonacci
(define (fib n)
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(fib 10)  ;; => 55

;; Mutual recursion
(define (my-even? n)
  (if (= n 0) true (my-odd? (- n 1))))
(define (my-odd? n)
  (if (= n 0) false (my-even? (- n 1))))
(my-even? 4)  ;; => true
(my-odd? 7)   ;; => true

;; loop/recur — tail-call optimized (no stack overflow)
;; loop creates named recursion points, recur jumps back
(define (factorial-tc n)
  (loop ((i n) (acc 1))
    (if (<= i 0)
      acc
      (recur (- i 1) (* acc i)))))
(factorial-tc 20)  ;; => 2432902008176640000

;; loop with multiple accumulators
(define (fib-tc n)
  (loop ((i n) (a 0) (b 1))
    (if (= i 0)
      a
      (recur (- i 1) b (+ a b)))))
(fib-tc 50)  ;; => 12586269025

;; Recursive list operations
(define (my-sum lst)
  (if (nil? lst) 0
    (+ (car lst) (my-sum (cdr lst)))))
(my-sum (list 1 2 3 4 5))  ;; => 15

;; TCO sum with loop/recur
(define (sum-tc lst)
  (loop ((remaining lst) (acc 0))
    (if (nil? remaining)
      acc
      (recur (cdr remaining) (+ acc (car remaining))))))
(sum-tc (list 1 2 3 4 5))  ;; => 15
