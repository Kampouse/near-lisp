;; 11-stdlib-crypto.lisp — (require "crypto")

(require "crypto")

;; SHA-256 hash
(sha256 "hello")              ;; => hex string of sha256("hello")
(hash/sha256-bytes "hello")   ;; same, alias

;; Keccak-256 hash
(keccak256 "hello")           ;; => hex string of keccak256("hello")
(hash/keccak256-bytes "hello") ;; same, alias

;; Practical: hash a structured value
(sha256 (str-concat "transfer:" (str-concat "from:" "to:100")))
