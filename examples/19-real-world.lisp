;; 19-real-world.lisp — practical patterns and recipes

;; === 1. Access control ===
(define owner "alice.near")
(define (owner-only? caller)
  (= caller owner))

;; === 2. Amount formatting ===
(require "string")
;; YOCTO per NEAR = 10^24 — beyond i64 range, so store as float
(define YOCTO 1000000000000000000000000.0)
(define (format-near yocto)
  (str-concat (to-string (/ yocto YOCTO)) " NEAR"))

;; === 3. Data validation ===
(define (valid-transfer? from to amount)
  (and
    (> (str-length from) 0)
    (> (str-length to) 0)
    (> amount 0)
    (!= from to)))

;; === 4. String templating ===
(define (template greeting name)
  (str-concat greeting ", " (str-concat name "!")))
(template "Hello" "NEAR")  ;; => "Hello, NEAR!"

;; === 5. List processing pipeline ===
(require "list")
(require "math")

(define scores (list 85 92 78 95 60 88 73))

;; Average score
(define avg (/ (reduce + 0 scores) (len scores)))

;; Passing scores (>= 70)
(define passing (filter (lambda (x) (>= x 70)) scores))

;; Top performers (>= 90)
(define top (filter (lambda (x) (>= x 90)) scores))

;; === 6. Simple finite state machine ===
(define (next-state current event)
  (cond
    ((and (= current "idle") (= event "start")) "running")
    ((and (= current "running") (= event "stop")) "stopped")
    ((and (= current "stopped") (= event "reset")) "idle")
    (true current)))

(next-state "idle" "start")      ;; => "running"
(next-state "running" "stop")    ;; => "stopped"
(next-state "stopped" "reset")   ;; => "idle"
(next-state "running" "reset")   ;; => "running" (no transition)

;; === 7. Compound policy ===
(define (approve-trade? buyer seller amount token)
  (and
    (valid-transfer? buyer seller amount)
    (<= amount 10000)
    (or (= token "near") (= token "usdt"))
    (!= buyer seller)))
