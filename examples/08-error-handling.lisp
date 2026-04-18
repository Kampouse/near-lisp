;; 08-error-handling.lisp — try/catch, error propagation

;; try catches errors and returns fallback
(try
  (/ 1 0)
  (catch e
    (str-concat "caught: " e)))

;; try without error returns the value
(try
  (+ 1 2)
  (catch e nil))

;; Custom error with error function
(define (safe-div a b)
  (if (= b 0)
    (error "division by zero")
    (/ a b)))

(try
  (safe-div 10 0)
  (catch e e))

;; Nested try/catch
(define (parse-positive s)
  (try
    (let ((n (to-num s)))
      (if (< n 0)
        (error "negative")
        n))
    (catch e -1)))

(parse-positive "42")
(parse-positive "-5")
(parse-positive "abc")
