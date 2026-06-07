(defmacro cond clauses
  (if (= clauses ())
      ()
      (if (= (car (car clauses)) 'else)
          (car (cdr (car clauses)))
          `(if ,(car (car clauses))
               ,(car (cdr (car clauses)))
               (cond ,@(cdr clauses))))))

(defmacro for (var seq . body)
  `(reduce (fn __for-fn (__acc ,var) (do ,@body)) () ,seq))

(defmacro loop (n . body)
  `(reduce (fn __loop-fn (__acc __i) (do ,@body)) () (range 0 ,n)))

(defmacro while (cond . body)
  `(do
     (let! __last ())
     ((fn __while-iter ()
        (if ,cond
            (do (set! __last (do ,@body))
                (__while-iter))
            (deref __last))))))

(defmacro or (a b)
    `(if ,a
       1
       (if ,b 1 0)))


(defmacro and (a b)
    `(if ,a
      (if ,b 1 0)
      0))

(defmacro compose fns
  (if (= fns ())
      `id
      (if (= (cdr fns) ())
          (car fns)
          `(fn __c (__x) (,(car fns) ((compose ,@(cdr fns)) __x))))))
