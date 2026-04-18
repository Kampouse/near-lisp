;; 10-stdlib-string.lisp — (require "string")

(require "string")

;; str-join — join list with separator
(str-join ", " (list "near" "lisp" "contract"))  ;; => "near, lisp, contract"

;; str-replace — replace all occurrences
(str-replace "hello world" "o" "0")  ;; => "hell0 w0rld"

;; str-repeat — repeat string n times
(str-repeat "ha" 3)  ;; => "hahaha"

;; str-pad-left / str-pad-right — padding
(str-pad-left "42" 5 "0")   ;; => "00042"
(str-pad-right "hi" 6 ".")  ;; => "hi...."

;; Combined with built-in string ops
(require "list")
(str-join ""
  (map (lambda (n) (str-pad-left (to-string n) 3 "0"))
       (range 1 6)))
;; => "001002003004005"
