;; 14-policies.lisp — on-chain policy engine

;; Policies are stored Lisp expressions that evaluate to true/false.
;; They're the core use case: authorization rules, compliance checks, etc.

;; === Save a policy (contract call: save_policy) ===
;; Policy: "score must be >= 50"
;;   save_policy({ name: "passing-grade", policy: "(>= score 50)" })

;; === Evaluate with input data ===
;; eval_policy({ name: "passing-grade", input_json: "{\"score\": 85}" })
;; => "true"

;; eval_policy({ name: "passing-grade", input_json: "{\"score\": 30}" })
;; => "false"

;; === Complex policy examples ===

;; Multi-field policy
;;   save_policy({ name: "kyc-check", policy: "
;;     (and
;;       (>= age 18)
;;       (= country \"CA\")
;;       (not (= status \"blocked\")))
;;   " })
;;   eval_policy({ name: "kyc-check", input_json: "{\"age\":25,\"country\":\"CA\",\"status\":\"active\"}" })
;;   => "true"

;; Policy using stdlib
;;   save_policy({ name: "whale-check", policy: "
;;     (require \"list\")
;;     (and
;;       (> amount 1000)
;;       (every (lambda (x) (> x 0)) amounts))
;;   " })

;; Policy with crypto
;;   save_policy({ name: "integrity-check", policy: "
;;     (= (sha256 data) expected-hash)
;;   " })
;;   eval_policy({ name: "integrity-check",
;;     input_json: "{\"data\":\"hello\",\"expected-hash\":\"<hash>\"}" })

;; === check_policy — returns boolean directly ===
;; check_policy({ policy: "(>= score 50)", input_json: "{\"score\":85}" })
;; => true (boolean, not string)

;; === Compose policies from modules ===
;;   save_module({ name: "policy-helpers", code: "
;;     (require \"list\")
;;     (define (all-positive? lst) (every (lambda (x) (> x 0)) lst))
;;     (define (in-range? n lo hi) (and (>= n lo) (<= n hi)))
;;   " })
;;   save_policy({ name: "valid-bid", policy: "
;;     (require \"policy-helpers\")
;;     (and
;;       (in-range? price 10 10000)
;;       (all-positive? quantities))
;;   " })
