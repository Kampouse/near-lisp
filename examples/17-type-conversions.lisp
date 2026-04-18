;; 17-type-conversions.lisp — to-string, to-num, type predicates

;; to-string — convert anything to string
(to-string 42)        ;; => "42"
(to-string true)      ;; => "true"
(to-string nil)       ;; => "nil"
(to-string "already") ;; => "already"
(to-string (list 1 2)) ;; => "(1 2)"

;; to-num — convert string/bool to number
(to-num "42")         ;; => 42
(to-num "3.14")       ;; => 3.14
(to-num true)         ;; => 1
(to-num false)        ;; => 0

;; nil? — check for nil
(nil? nil)            ;; => true
(nil? 0)              ;; => false
(nil? (list))         ;; => false

;; type? — get type name
(type? 42)            ;; => "number"
(type? 3.14)          ;; => "number"
(type? "hello")       ;; => "string"
(type? true)          ;; => "boolean"
(type? (list 1 2))    ;; => "list"
(type? nil)           ;; => "nil"
(type? (lambda (x) x)) ;; => "lambda"

;; Practical: safe parsing pipeline
(define (parse-int s)
  (try
    (to-num s)
    (catch e 0)))
(parse-int "123")  ;; => 123
(parse-int "bad")  ;; => 0
