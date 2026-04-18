;; 16-cross-contract.lisp — async calls to other NEAR contracts

;; Cross-contract calls (ccall) are async — they yield execution and resume
;; when the result comes back. The contract state is serialized via Borsh.

;; Basic ccall: call a view method on another contract
;; (ccall "social.near" "get_status" "{\"account_id\":\"alice.near\"}")

;; The result is available via near/ccall-result in the continuation
;; (near/ccall-result)  ;; => the result of the ccall

;; Batch calls — multiple ccalls, then process all results
;; (define results
;;   (progn
;;     (ccall "contract-a.near" "get_value" "{}")
;;     (ccall "contract-b.near" "get_price" "{\"token\":\"near\"}")
;;     (near/batch-result)))
;; results  ;; => list of results

;; Count pending results
;; (near/ccall-count)  ;; => number of pending ccall results

;; === Example: Price oracle policy ===
;; This policy fetches a price from an oracle and checks a threshold:
;;
;; (define oracle-price
;;   (ccall "price-oracle.near" "get_price" "{\"token\":\"near\"}"))
;; (>= (to-num (near/ccall-result)) 500)
;;
;; Save as policy, evaluate on-chain — the contract will yield/resume
;; automatically to fetch the price.
