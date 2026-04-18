;; 12-near-context.lisp — blockchain context builtins

;; Block info
near/block-height             ;; => current block height (number)
near/timestamp                ;; => block timestamp in nanoseconds (number)

;; Account info
near/predecessor              ;; => caller account ID (string)
near/signer                   ;; => signing account ID (string)
near/account-balance          ;; => contract balance in yoctoNEAR (string)
near/attached-deposit         ;; => attached deposit in yoctoNEAR (string)
near/account-locked-balance   ;; => locked balance (string)

;; Logging
(near/log "Hello from on-chain Lisp!")  ;; emits log event

;; Storage — persistent key-value per caller
(near/storage-write "my-key" "my-value")  ;; => true
(near/storage-read "my-key")               ;; => "my-value"
(near/storage-read "nonexistent")           ;; => nil

;; Practical: store computed values
(define (save-score name score)
  (near/storage-write
    (str-concat "score:" name)
    (to-string score)))
(save-score "alice" 95)
(near/storage-read "score:alice")  ;; => "95"
