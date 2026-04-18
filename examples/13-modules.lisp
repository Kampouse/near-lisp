;; 13-modules.lisp — custom modules with require
;;
;; Modules are saved on-chain via save_module, then loaded with require.
;; Stdlib modules (math, list, string, crypto) are built-in.
;; Custom modules must be registered first via save_module().

;; === Built-in stdlib module ===
(require "math")
(abs -5)         ;; => 5
(min 3 7)        ;; => 3
(max 3 7)        ;; => 7

;; === Custom module pattern ===
;; On-chain, you'd save this as a module:
;;   save_module("token-utils", "(define (format-amount yocto) ...)")
;; Then load with (require "token-utils")
;;
;; For this example, we define the functions inline.
;; Note: yoctoNEAR amounts are strings since they exceed i64 range.
(define (valid-account? id)
  (and (> (str-length id) 0) (str-contains id ".")))

(define (transfer-msg from to amount)
  (str-concat from " -> " to ": " amount " yoctoNEAR"))

(valid-account? "user.near")    ;; => true
(valid-account? "")             ;; => false
(transfer-msg "alice.near" "bob.near" "5000000000000000000000000")

;; === Modules can use stdlib ===
(define (fee amount rate)
  (abs (/ (* amount rate) 100)))
(fee 1000 3)  ;; => 30
