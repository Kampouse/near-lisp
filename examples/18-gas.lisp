;; 18-gas.lisp — gas management and limits

;; Every operation costs gas. Complex computations will exhaust gas.
;; The contract has a configurable eval_gas_limit (set at init).

;; Simple expressions use minimal gas
(+ 1 2)  ;; fast

;; Deep recursion burns gas fast
;; This would run out of gas with default limit:
;; (define (countdown n) (if (= n 0) 0 (countdown (- n 1))))
;; (countdown 100000)  ;; => out of gas

;; loop/recur is more gas-efficient than naive recursion
(define (count-tc n)
  (loop ((i n) (acc 0))
    (if (= i 0)
      acc
      (recur (- i 1) (+ acc 1)))))
(count-tc 100)  ;; => 100

;; Gas exhaustion is catchable with try/catch
(try
  (count-tc 999999)
  (catch e
    (if (str-contains e "gas")
      "ran out of gas"
      e)))

;; Tips for gas efficiency:
;; 1. Use loop/recur instead of naive recursion
;; 2. Avoid deeply nested list operations
;; 3. Cache module loads — require is free if already loaded
;; 4. Keep policy expressions simple
