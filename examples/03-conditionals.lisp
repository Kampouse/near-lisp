;; 03-conditionals.lisp — if, cond, and, or, not

;; Simple if
(if true "yes" "no")            ;; => "yes"
(if (> 5 3) "bigger" "smaller") ;; => "bigger"

;; if without else
(if false "never")               ;; => nil

;; Nested if
(define (classify n)
  (if (< n 0) "negative"
    (if (= n 0) "zero" "positive")))
(classify -5)   ;; => "negative"
(classify 0)    ;; => "zero"
(classify 42)   ;; => "positive"

;; cond — multi-branch
(define (grade score)
  (cond
    ((>= score 90) "A")
    ((>= score 80) "B")
    ((>= score 70) "C")
    ((>= score 60) "D")
    (true "F")))
(grade 95)  ;; => "A"
(grade 72)  ;; => "C"

;; Logical operators
(and true true)       ;; => true
(and true false)      ;; => false
(or false true)       ;; => true
(or false false)      ;; => false
(not true)            ;; => false

;; Short-circuit: and/or return the deciding value
(and 1 2 3)           ;; => 3
(or nil "default")    ;; => "default"
